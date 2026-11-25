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
            ..valid
        },
    ] {
        assert!(!invalid.is_valid(), "{invalid:?}");
    }
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
