use super::*;

const MAC: LocalMachine = LocalMachine {
    hostname: None,
    machine_id: None,
};

fn local() -> LocalMachine {
    LocalMachine {
        hostname: Some("mac".to_owned()),
        ..MAC
    }
}

fn start(contexts: &mut Contexts, id: &str, fields: &str, group: u32) {
    contexts.apply(
        ContextSignal {
            action: ContextAction::Start,
            id: id.as_bytes().to_vec(),
            fields: fields.as_bytes().to_vec(),
        },
        Some(group),
    );
}

fn end(contexts: &mut Contexts, id: &str) {
    contexts.apply(
        ContextSignal {
            action: ContextAction::End,
            id: id.as_bytes().to_vec(),
            fields: Vec::new(),
        },
        None,
    );
}

fn place(kind: PlaceKind, name: &str) -> ContextPlace {
    ContextPlace {
        kind,
        name: name.to_owned(),
    }
}

#[test]
fn a_shell_of_another_computer_names_that_computer() {
    let mut contexts = Contexts::default();
    start(
        &mut contexts,
        "local",
        "type=shell;user=alice;hostname=mac",
        10,
    );
    assert_eq!(contexts.summary(&local()), None);

    start(
        &mut contexts,
        "remote",
        "type=shell;user=alice;hostname=build;machineid=0123",
        20,
    );
    assert_eq!(
        contexts.summary(&local()),
        Some(TerminalContext {
            place: Some(place(PlaceKind::Remote, "build")),
            root: false,
        })
    );
}

#[test]
fn the_machine_id_decides_before_the_host_name() {
    let ours = LocalMachine {
        hostname: Some("ubuntu".to_owned()),
        machine_id: Some("aaaa".to_owned()),
    };
    let mut contexts = Contexts::default();
    start(
        &mut contexts,
        "same",
        "type=shell;hostname=ubuntu;machineid=AAAA",
        10,
    );
    assert_eq!(contexts.summary(&ours), None);

    start(
        &mut contexts,
        "other",
        "type=shell;hostname=ubuntu;machineid=bbbb",
        20,
    );
    assert_eq!(
        contexts.summary(&ours).and_then(|context| context.place),
        Some(place(PlaceKind::Remote, "ubuntu"))
    );
}

#[test]
fn an_elevated_context_runs_as_root_until_it_ends() {
    let mut contexts = Contexts::default();
    start(
        &mut contexts,
        "run0",
        "type=elevate;user=alice;targetuser=root",
        10,
    );
    assert_eq!(
        contexts.summary(&local()),
        Some(TerminalContext {
            place: None,
            root: true,
        })
    );

    end(&mut contexts, "run0");
    assert_eq!(contexts.summary(&local()), None);
}

#[test]
fn a_shell_inside_a_container_names_the_container() {
    let mut contexts = Contexts::default();
    start(
        &mut contexts,
        "nspawn",
        "type=container;container=debian;targetuser=root",
        10,
    );
    start(
        &mut contexts,
        "shell",
        "type=shell;user=root;hostname=debian-box;machineid=4567",
        10,
    );

    assert_eq!(
        contexts.summary(&local()),
        Some(TerminalContext {
            place: Some(place(PlaceKind::Container, "debian-box")),
            root: true,
        })
    );
}

#[test]
fn an_end_closes_the_contexts_inside_and_a_new_start_replaces_them() {
    let mut contexts = Contexts::default();
    start(&mut contexts, "a", "type=remote;targethost=one", 10);
    start(&mut contexts, "b", "type=elevate;targetuser=root", 10);
    end(&mut contexts, "a");
    assert_eq!(contexts.summary(&local()), None);

    start(&mut contexts, "a", "type=remote;targethost=one", 10);
    start(&mut contexts, "b", "type=elevate;targetuser=root", 10);
    start(&mut contexts, "a", "type=remote;targethost=two", 10);
    assert_eq!(
        contexts.summary(&local()),
        Some(TerminalContext {
            place: Some(place(PlaceKind::Remote, "two")),
            root: false,
        })
    );
}

#[test]
fn the_shell_in_front_ends_the_contexts_of_its_jobs() {
    let mut contexts = Contexts::default();
    start(&mut contexts, "shell", "type=shell;hostname=mac", 10);
    start(&mut contexts, "ssh", "type=shell;hostname=build", 20);
    start(&mut contexts, "root", "type=elevate;targetuser=root", 20);

    contexts.shell_in_front(10);
    assert_eq!(contexts.summary(&local()), None);
    assert_eq!(contexts.stack.len(), 1);

    start(&mut contexts, "ssh", "type=shell;hostname=build", 20);
    contexts.shell_in_front(30);
    assert!(contexts.stack.is_empty());
}

#[test]
fn values_are_unescaped_and_invalid_values_or_identifiers_are_ignored() {
    let mut contexts = Contexts::default();
    start(
        &mut contexts,
        "a",
        r"type=remote;targethost=a\x3bb\x5cc",
        10,
    );
    assert_eq!(
        contexts.summary(&local()).and_then(|context| context.place),
        Some(place(PlaceKind::Remote, r"a;b\c"))
    );

    let mut contexts = Contexts::default();
    start(
        &mut contexts,
        "a",
        "type=remote;targethost=bad\u{7}name",
        10,
    );
    start(
        &mut contexts,
        "b",
        &format!("type=remote;targethost={}", "x".repeat(256)),
        10,
    );
    assert_eq!(contexts.summary(&local()), None);
    assert_eq!(contexts.stack.len(), 2);

    for id in [String::new(), "x".repeat(65), "tab\tid".to_owned()] {
        start(&mut contexts, &id, "type=elevate;targetuser=root", 10);
    }
    assert_eq!(contexts.stack.len(), 2);
}

#[test]
fn contexts_beyond_the_depth_limit_are_ignored() {
    let mut contexts = Contexts::default();
    for index in 0..MAX_CONTEXTS {
        start(
            &mut contexts,
            &index.to_string(),
            "type=shell;hostname=mac",
            10,
        );
    }
    start(&mut contexts, "last", "type=elevate;targetuser=root", 10);

    assert_eq!(contexts.stack.len(), MAX_CONTEXTS);
    assert_eq!(contexts.summary(&local()), None);
}
