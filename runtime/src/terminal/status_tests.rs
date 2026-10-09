use super::*;

fn report(id: &str, state: ProgramState) -> ProgramStatusReport {
    ProgramStatusReport {
        id: id.to_owned(),
        state: Some(state),
        kind: None,
        progress: None,
        app: None,
        title: None,
        message: None,
    }
}

fn clear(id: &str) -> ProgramStatusReport {
    ProgramStatusReport {
        state: None,
        ..report(id, ProgramState::Idle)
    }
}

fn state(records: &ProgramStatusRecords) -> Option<ProgramState> {
    records.summary().map(|status| status.state)
}

#[test]
fn a_report_replaces_its_record_completely() {
    let mut records = ProgramStatusRecords::default();
    records.apply(ProgramStatusReport {
        kind: Some(BlockedKind::Permission),
        progress: Some(40),
        app: Some("claude-code".to_owned()),
        message: Some("Allow the command?".to_owned()),
        ..report("", ProgramState::Blocked)
    });
    records.apply(report("", ProgramState::Working));

    assert_eq!(
        records.summary(),
        Some(ProgramStatus {
            state: ProgramState::Working,
            kind: None,
            progress: None,
            app: None,
            title: None,
            message: None,
        })
    );
}

#[test]
fn the_most_urgent_record_is_the_status_of_the_terminal() {
    let mut records = ProgramStatusRecords::default();
    records.apply(report("", ProgramState::Idle));
    assert_eq!(state(&records), Some(ProgramState::Idle));
    records.apply(report("lint", ProgramState::Done));
    assert_eq!(state(&records), Some(ProgramState::Done));
    records.apply(report("tests", ProgramState::Working));
    assert_eq!(state(&records), Some(ProgramState::Working));
    records.apply(report("deploy", ProgramState::Error));
    assert_eq!(state(&records), Some(ProgramState::Error));
    records.apply(ProgramStatusReport {
        kind: Some(BlockedKind::Question),
        title: Some("Review the plan".to_owned()),
        ..report("plan", ProgramState::Blocked)
    });
    records.apply(report("docs", ProgramState::Working));

    let status = records.summary().expect("status");
    assert_eq!(status.state, ProgramState::Blocked);
    assert_eq!(status.kind, Some(BlockedKind::Question));
    assert_eq!(status.title.as_deref(), Some("Review the plan"));
}

#[test]
fn the_newer_of_two_equal_records_is_the_status() {
    let mut records = ProgramStatusRecords::default();
    records.apply(ProgramStatusReport {
        title: Some("first".to_owned()),
        ..report("a", ProgramState::Working)
    });
    records.apply(ProgramStatusReport {
        title: Some("second".to_owned()),
        ..report("b", ProgramState::Working)
    });
    assert_eq!(
        records.summary().and_then(|status| status.title).as_deref(),
        Some("second")
    );
    records.apply(ProgramStatusReport {
        title: Some("first again".to_owned()),
        ..report("a", ProgramState::Working)
    });
    assert_eq!(
        records.summary().and_then(|status| status.title).as_deref(),
        Some("first again")
    );
}

#[test]
fn clear_removes_a_record_and_the_records_below_it() {
    let mut records = ProgramStatusRecords::default();
    records.apply(report("build", ProgramState::Working));
    records.apply(report("build/test", ProgramState::Blocked));
    records.apply(report("builder", ProgramState::Done));
    records.apply(clear("build"));
    assert_eq!(state(&records), Some(ProgramState::Done));

    records.apply(report("", ProgramState::Working));
    records.apply(clear(""));
    assert_eq!(records.summary(), None);
}

#[test]
fn the_end_of_the_program_keeps_only_results() {
    let mut records = ProgramStatusRecords::default();
    records.apply(report("", ProgramState::Idle));
    records.apply(report("a", ProgramState::Working));
    records.apply(report("b", ProgramState::Blocked));
    records.apply(report("c", ProgramState::Done));
    records.program_exited();
    assert_eq!(state(&records), Some(ProgramState::Done));

    records.apply(report("d", ProgramState::Error));
    records.program_exited();
    assert_eq!(state(&records), Some(ProgramState::Error));
}

#[test]
fn a_record_takes_the_program_name_of_its_nearest_parent() {
    let mut records = ProgramStatusRecords::default();
    records.apply(ProgramStatusReport {
        app: Some("claude-code".to_owned()),
        ..report("", ProgramState::Idle)
    });
    records.apply(report("tasks/review", ProgramState::Working));
    assert_eq!(
        records.summary().and_then(|status| status.app).as_deref(),
        Some("claude-code")
    );

    records.apply(ProgramStatusReport {
        app: Some("cargo".to_owned()),
        ..report("tasks", ProgramState::Idle)
    });
    assert_eq!(
        records.summary().and_then(|status| status.app).as_deref(),
        Some("cargo")
    );

    records.apply(report("tasksuite", ProgramState::Blocked));
    assert_eq!(
        records.summary().and_then(|status| status.app).as_deref(),
        Some("claude-code")
    );
}

#[test]
fn the_oldest_record_makes_room_for_a_new_one() {
    let mut records = ProgramStatusRecords::default();
    records.apply(report("first", ProgramState::Blocked));
    for index in 0..MAX_RECORDS - 1 {
        records.apply(report(&format!("task-{index}"), ProgramState::Working));
    }
    assert_eq!(state(&records), Some(ProgramState::Blocked));
    records.apply(report("one-more", ProgramState::Working));
    assert_eq!(state(&records), Some(ProgramState::Working));
    assert_eq!(records.records.len(), MAX_RECORDS);
}

#[test]
fn values_that_do_not_belong_to_a_state_are_dropped() {
    let mut records = ProgramStatusRecords::default();
    records.apply(ProgramStatusReport {
        kind: Some(BlockedKind::Auth),
        progress: Some(10),
        app: Some("not a name".to_owned()),
        ..report("", ProgramState::Done)
    });
    let status = records.summary().expect("status");
    assert_eq!(
        (status.kind, status.progress, status.app.as_deref()),
        (None, None, None)
    );
    assert!(status.is_valid());

    records.apply(ProgramStatusReport {
        progress: Some(101),
        ..report("", ProgramState::Working)
    });
    assert_eq!(records.summary().and_then(|status| status.progress), None);
}

#[test]
fn text_for_display_has_no_hidden_characters_and_a_limited_length() {
    let mut records = ProgramStatusRecords::default();
    records.apply(ProgramStatusReport {
        title: Some("\u{202E}  \u{200B}".to_owned()),
        message: Some(format!(" safe\u{202E}text\u{2066}{} ", "x".repeat(2_000))),
        ..report("", ProgramState::Blocked)
    });
    let status = records.summary().expect("status");
    assert_eq!(status.title, None);
    let message = status.message.as_deref().expect("message");
    assert!(message.starts_with("safetextxxx"));
    assert_eq!(message.chars().count(), MAX_MESSAGE_CHARS - 1);
    assert!(status.is_valid());
}

#[test]
fn a_status_from_another_computer_must_follow_the_same_rules() {
    let valid = ProgramStatus {
        state: ProgramState::Blocked,
        kind: Some(BlockedKind::Permission),
        progress: Some(100),
        app: Some("claude-code".to_owned()),
        title: Some("Review".to_owned()),
        message: Some("Allow the command?".to_owned()),
    };
    assert!(valid.is_valid());
    for invalid in [
        ProgramStatus {
            state: ProgramState::Working,
            ..valid.clone()
        },
        ProgramStatus {
            progress: Some(101),
            ..valid.clone()
        },
        ProgramStatus {
            app: Some("a b".to_owned()),
            ..valid.clone()
        },
        ProgramStatus {
            title: Some("x".repeat(MAX_TITLE_CHARS + 1)),
            ..valid.clone()
        },
        ProgramStatus {
            message: Some("line\nbreak".to_owned()),
            ..valid.clone()
        },
        ProgramStatus {
            message: Some("hidden\u{202E}".to_owned()),
            ..valid.clone()
        },
        ProgramStatus {
            message: Some(String::new()),
            ..valid.clone()
        },
    ] {
        assert!(!invalid.is_valid(), "{invalid:?}");
    }
    assert!(serde_json::from_str::<ProgramStatus>(r#"{"state":"idle","other":1}"#).is_err());
    assert_eq!(
        serde_json::to_string(&ProgramStatus {
            kind: None,
            progress: None,
            app: None,
            title: None,
            message: None,
            ..valid
        })
        .expect("json"),
        r#"{"state":"blocked"}"#
    );
}

#[test]
fn a_progress_bar_is_the_status_until_the_program_reports_its_own() {
    let progress = |state, value| ProgramStatusReport {
        progress: value,
        ..report("", state)
    };
    let mut records = ProgramStatusRecords::default();
    records.apply_progress(progress(ProgramState::Working, Some(40)));
    assert_eq!(
        records
            .summary()
            .map(|status| (status.state, status.progress)),
        Some((ProgramState::Working, Some(40)))
    );
    records.apply_progress(clear(""));
    assert_eq!(records.summary(), None);

    records.apply(ProgramStatusReport {
        kind: Some(BlockedKind::Permission),
        message: Some("Allow the command?".to_owned()),
        ..report("", ProgramState::Blocked)
    });
    records.apply_progress(progress(ProgramState::Working, None));
    records.apply_progress(clear(""));
    let status = records.summary().expect("status");
    assert_eq!(status.state, ProgramState::Blocked);
    assert_eq!(status.message.as_deref(), Some("Allow the command?"));

    records.program_exited();
    records.apply_progress(progress(ProgramState::Working, Some(10)));
    assert_eq!(state(&records), Some(ProgramState::Working));

    records.apply(clear(""));
    records.apply_progress(progress(ProgramState::Error, None));
    assert_eq!(records.summary(), None);
    records.reset();
    records.apply_progress(progress(ProgramState::Error, None));
    assert_eq!(state(&records), Some(ProgramState::Error));
}
