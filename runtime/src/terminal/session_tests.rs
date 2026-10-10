use super::super::{BlockedKind, ProgramState, ProgramStatus};
use super::*;

async fn shell(
    script: &str,
) -> (
    LocalSession,
    mpsc::Receiver<SessionChange>,
    tempfile::TempDir,
) {
    let root = tempfile::tempdir().expect("temporary cwd");
    let directory = root.path().canonicalize().expect("canonical cwd");
    let (changes, receiver) = mpsc::channel(128);
    let session = LocalSession::spawn(
        Uuid::now_v7(),
        Uuid::now_v7(),
        PathBuf::from("/bin/sh"),
        vec!["-c".to_owned(), script.to_owned()],
        directory,
        true,
        changes,
    )
    .await
    .expect("real shell");
    (session, receiver, root)
}

async fn wait_text(subscription: &mut Subscription, text: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = subscription
                .data
                .recv()
                .await
                .expect("terminal data before marker");
            bytes.extend_from_slice(&frame.bytes);
            if bytes.windows(text.len()).any(|window| window == text) {
                break;
            }
        }
    })
    .await;
    assert!(outcome.is_ok(), "missing {text:?}; received {bytes:?}");
    bytes
}

#[tokio::test]
async fn launch_exposes_its_terminal_context_and_preserves_terminal_setup() {
    let (session, mut changes, root) = shell(
        r#"while [ ! -f ready ]; do sleep 0.01; done
printf '%s' "${KODOSI_SESSION_ID-}" > session-id
printf '%s' "$TERM" > term
pwd -P > cwd
stty size > size
stty -echo
printf LAUNCH-READY
exec /bin/cat"#,
    )
    .await;
    let mut subscription = session.subscribe().await.expect("subscribe");
    std::fs::write(root.path().join("ready"), b"").expect("release shell");
    wait_text(&mut subscription, b"LAUNCH-READY").await;
    session
        .input(
            subscription.connection_id,
            Bytes::from_static(b"launch-input\n"),
        )
        .expect("input");
    wait_text(&mut subscription, b"launch-input").await;
    session.close().await.expect("close");

    let marker = std::fs::read_to_string(root.path().join("session-id")).expect("terminal context");
    let actual = Uuid::parse_str(&marker).expect("a terminal identifier");
    let ended = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(change) = changes.recv().await {
            if let SessionChange::Ended { id, .. } = change {
                return id;
            }
        }
        panic!("terminal ended without its identity");
    })
    .await
    .expect("terminal completion");
    assert_eq!(actual, ended);
    assert_eq!(
        std::fs::read(root.path().join("term")).expect("terminal type"),
        b"xterm-256color"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("cwd")).expect("working directory"),
        format!(
            "{}\n",
            root.path().canonicalize().expect("canonical cwd").display()
        )
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("size")).expect("terminal size"),
        "32 120\n"
    );
}

#[tokio::test]
async fn invalid_geometry_is_rejected_without_stopping_the_process() {
    let (session, _changes, _root) = shell("stty -echo; exec /bin/cat").await;
    let mut subscription = session.subscribe().await.expect("subscribe");
    let size = TerminalSize::new(20, 80).expect("size");
    let invalid = TerminalPixelGeometry::new(800, 400, 11, 20).expect("nonzero geometry");
    assert!(matches!(
        session
            .resize(subscription.connection_id, size, Some(invalid), true)
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(!session.is_closed());
    session
        .input(
            subscription.connection_id,
            Bytes::from_static(b"after-rejected-resize\n"),
        )
        .expect("input");
    wait_text(&mut subscription, b"after-rejected-resize").await;
    let cut = session
        .checkpoint(subscription.connection_id)
        .await
        .expect("checkpoint");
    assert_eq!(cut.checkpoint.size(), TerminalSize::default());
    session.close().await.expect("close");
}

#[tokio::test]
async fn resize_claim_is_current_connection_ownership_not_a_permission_role() {
    let (session, _changes, _root) = shell("exec /bin/cat").await;
    let first = session.subscribe().await.expect("first");
    let second = session.subscribe().await.expect("second");
    let size = TerminalSize::new(18, 60).expect("size");
    session
        .resize(first.connection_id, size, None, false)
        .await
        .expect("first size owner");
    assert!(
        session
            .resize(second.connection_id, size, None, false)
            .await
            .is_err()
    );
    session
        .resize(second.connection_id, size, None, true)
        .await
        .expect("any controller may claim");
    drop(second);
    session
        .resize(first.connection_id, size, None, false)
        .await
        .expect("closed owner released");
    session.close().await.expect("close");
}

#[tokio::test]
async fn dropping_a_view_releases_aggregated_focus_without_explicit_unsubscribe() {
    let (session, _changes, root) = shell(
        r"stty raw -echo; while [ ! -f ready ]; do sleep 0.01; done; printf '\033[?1004hREADY'; exec /bin/cat",
    )
    .await;
    let mut observer = session.subscribe().await.expect("observer");
    let first = session.subscribe().await.expect("first");
    let second = session.subscribe().await.expect("second");
    std::fs::write(root.path().join("ready"), b"").expect("release fixture output");
    wait_text(&mut observer, b"READY").await;
    session
        .focus(first.connection_id, true)
        .await
        .expect("focus first");
    wait_text(&mut observer, b"\x1b[I").await;
    session
        .focus(second.connection_id, true)
        .await
        .expect("focus second");
    drop(first);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(observer.data.try_recv().is_err());
    drop(second);
    wait_text(&mut observer, b"\x1b[O").await;
    session.close().await.expect("close");
}

#[tokio::test]
async fn canceled_host_authorization_cannot_resize_focus_or_close() {
    let (session, _changes, _root) = shell("exec /bin/cat").await;
    let subscription = session.subscribe().await.expect("subscribe");
    let authorization = CancellationToken::new();
    authorization.cancel();
    let connection_id = Uuid::now_v7();
    for control in [
        TerminalControl::Close,
        TerminalControl::Focus { focused: true },
        TerminalControl::Resize {
            request_id: Uuid::now_v7().to_string(),
            rows: 10,
            cols: 10,
            width_pixels: 0,
            height_pixels: 0,
            cell_width_pixels: 0,
            cell_height_pixels: 0,
            claim: true,
        },
    ] {
        let (reply, result) = oneshot::channel();
        session
            .host_requests
            .send(HostRequest::Control {
                sender_user_id: "friend".to_owned(),
                sender_device_id: "device".to_owned(),
                connection_id,
                control,
                authorization: authorization.clone(),
                reply,
            })
            .await
            .expect("host request");
        assert!(result.await.expect("rejection").is_err());
    }
    assert_eq!(
        session
            .checkpoint(subscription.connection_id)
            .await
            .expect("checkpoint")
            .checkpoint
            .size(),
        TerminalSize::default()
    );
    session.close().await.expect("close");
}

#[tokio::test]
async fn resize_checkpoint_precedes_bootstrap_barrier_at_the_same_output_cut() {
    let (session, _changes, _root) = shell("sleep 30").await;
    let subscription = session.subscribe().await.expect("subscribe");
    let mut output = session.output();
    let size = TerminalSize::new(16, 48).expect("size");
    session
        .resize(subscription.connection_id, size, None, true)
        .await
        .expect("resize");
    let request_id = Uuid::now_v7();
    let (reply, result) = oneshot::channel();
    session
        .host_requests
        .send(HostRequest::Bootstrap {
            request_id,
            history: true,
            reply,
        })
        .await
        .expect("bootstrap");
    let cut = result.await.expect("bootstrap reply").expect("cut");
    let mut resized = false;
    loop {
        match output.recv().await.expect("ordered frame") {
            PublishedFrame::Resize {
                rows,
                cols,
                at_sequence,
            } => {
                assert_eq!((rows, cols), (size.rows(), size.cols()));
                assert_eq!(at_sequence, cut.next_sequence);
                resized = true;
            }
            PublishedFrame::BootstrapBarrier {
                request_id: received,
            } if received == request_id => {
                assert!(resized);
                break;
            }
            PublishedFrame::Raw { .. } | PublishedFrame::Metadata(_) => {}
            _ => panic!("unexpected publication frame"),
        }
    }
    session.close().await.expect("close");
}

#[tokio::test]
async fn simultaneous_close_waiters_confirm_reaping_even_with_full_admission_queue() {
    let (session, mut changes, root) = shell(
        "trap '' HUP TERM; while [ ! -f ready ]; do sleep 0.01; done; printf READY; sleep 30",
    )
    .await;
    let mut subscription = session.subscribe().await.expect("subscribe");
    std::fs::write(root.path().join("ready"), b"").expect("release resistant shell");
    wait_text(&mut subscription, b"READY").await;
    for _ in 0..COMMAND_CAPACITY * 2 {
        drop(session.input(subscription.connection_id, Bytes::from_static(b"x")));
    }
    let close_one = session.close();
    let close_two = session.close();
    assert!(session.is_closed());
    assert!(
        session
            .input(subscription.connection_id, Bytes::from_static(b"late"))
            .is_err()
    );
    let (first, second) = tokio::join!(close_one, close_two);
    first.expect("first waiter");
    second.expect("second waiter");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(changes.recv().await, Some(SessionChange::Ended { .. })) {
                break;
            }
        }
    })
    .await
    .expect("ended notification");
}

#[test]
fn byte_budget_covers_admitted_input_before_the_actor_polls() {
    let budget = Arc::new(Semaphore::new(MAX_PENDING_BYTES));
    let writes = (0..4)
        .map(|_| {
            PendingWrite::new(Bytes::from(vec![b'x'; MAX_INPUT_BYTES]), &budget, None)
                .expect("budget")
        })
        .collect::<Vec<_>>();
    assert!(PendingWrite::new(Bytes::from_static(b"x"), &budget, None).is_err());
    drop(writes);
    assert_eq!(budget.available_permits(), MAX_PENDING_BYTES);
}

#[tokio::test]
#[expect(
    clippy::significant_drop_tightening,
    reason = "actor.run consumes the actor and reaps the real PTY; there is no value left to drop"
)]
async fn revoked_pending_input_is_discarded_before_a_real_pty_write() {
    let root = tempfile::tempdir().expect("cwd");
    let directory = root.path().canonicalize().expect("canonical");
    let emulator = SessionTerminalHandle::spawn(
        TerminalSize::default(),
        TerminalHistoryPolicy::default(),
        true,
    )
    .expect("emulator");
    let (pty, reader) = KodosiPty::spawn_program(
        std::path::Path::new("/bin/sh"),
        &["-c".to_owned(), "stty raw -echo; exec /bin/cat".to_owned()],
        directory.to_str(),
        32,
        120,
    )
    .expect("pty");
    let (_commands, command_rx) = mpsc::channel(1);
    let (_host, host_rx) = mpsc::channel(1);
    let (output, _) = broadcast::channel(16);
    let (changes, _) = mpsc::channel(16);
    let (completed, _) = watch::channel(None);
    let authorization = CancellationToken::new();
    let mut actor = LocalActor {
        id: Uuid::now_v7(),
        incarnation: Uuid::now_v7(),
        pty,
        reader,
        emulator,
        commands: command_rx,
        host: host_rx,
        output,
        changes,
        cancellation: CancellationToken::new(),
        completed,
        input_budget: Arc::new(Semaphore::new(MAX_PENDING_BYTES)),
        subscribers: Subscribers::default(),
        queue: VecDeque::new(),
        focused: HashMap::new(),
        resize_owner: None,
        sequence: 0,
        host_stop_reply: None,
        program: None,
        prompt: false,
        viewers: HashMap::new(),
        working_directory: None,
        title: None,
        published_title: None,
        status: ProgramStatusRecords::default(),
        status_program: None,
        published_status: None,
        metadata_dirty: false,
        next_program_check: tokio::time::Instant::now(),
        next_refresh: tokio::time::Instant::now(),
    };
    actor
        .enqueue(
            Bytes::from_static(b"REVOKED\n"),
            Some(authorization.clone()),
        )
        .expect("queued input");
    authorization.cancel();
    actor.flush().expect("flush excludes revoked input");
    assert!(actor.queue.is_empty());
    assert_eq!(actor.input_budget.available_permits(), MAX_PENDING_BYTES);
    actor
        .enqueue(Bytes::from_static(b"ALLOWED\n"), None)
        .expect("allowed input");
    actor.flush().expect("write allowed bytes");
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    tokio::time::timeout(Duration::from_secs(2), async {
        while !captured.windows(7).any(|slice| slice == b"ALLOWED") {
            let count = actor.reader.read(&mut buffer).await.expect("read pty");
            assert!(count > 0);
            captured.extend_from_slice(&buffer[..count]);
        }
    })
    .await
    .expect("allowed output");
    assert!(!captured.windows(7).any(|slice| slice == b"REVOKED"));
    actor.cancellation.cancel();
    actor.run().await;
}

async fn next_status(changes: &mut mpsc::Receiver<SessionChange>) -> Option<ProgramStatus> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(SessionChange::Status { status, .. }) = changes.recv().await {
                break status;
            }
        }
    })
    .await
    .expect("program status change")
}

#[tokio::test]
async fn a_program_status_report_reaches_the_host_and_the_people_who_view_the_terminal() {
    let (session, mut changes, _root) = shell(
        "printf '\\033]7501;state=blocked:kind=permission:app=claude-code:msg=QWxsb3c/\\007'; exec /bin/cat",
    )
    .await;
    let mut output = session.output();
    let expected = ProgramStatus {
        state: ProgramState::Blocked,
        kind: Some(BlockedKind::Permission),
        progress: None,
        app: Some("claude-code".to_owned()),
        title: None,
        message: Some("Allow?".to_owned()),
    };
    assert_eq!(next_status(&mut changes).await, Some(expected.clone()));
    let metadata = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(PublishedFrame::Metadata(metadata)) = output.recv().await
                && metadata.status.is_some()
            {
                break metadata;
            }
        }
    })
    .await
    .expect("metadata for viewers");
    assert_eq!(metadata.status, Some(expected));
    assert!(metadata.is_valid());
    session.close().await.expect("close");
}

#[tokio::test]
async fn a_program_that_ends_while_it_works_leaves_no_status() {
    let (session, mut changes, _root) = shell(
        "set -m; /bin/sh -c \"printf '\\033]7501;state=working:progress=40\\007'; sleep 1\"; exec /bin/cat",
    )
    .await;
    assert_eq!(
        next_status(&mut changes)
            .await
            .map(|status| (status.state, status.progress)),
        Some((ProgramState::Working, Some(40)))
    );
    assert_eq!(next_status(&mut changes).await, None);
    session.close().await.expect("close");
}

#[tokio::test]
async fn a_progress_bar_shows_as_work_and_ends_with_its_program() {
    let (session, mut changes, _root) =
        shell("set -m; /bin/sh -c \"printf '\\033]9;4;1;40\\007'; sleep 1\"; exec /bin/cat").await;
    assert_eq!(
        next_status(&mut changes).await,
        Some(ProgramStatus {
            state: ProgramState::Working,
            kind: None,
            progress: Some(40),
            app: None,
            title: None,
            message: None,
        })
    );
    assert_eq!(next_status(&mut changes).await, None);
    session.close().await.expect("close");
}

async fn next_prompt(changes: &mut mpsc::Receiver<SessionChange>) -> bool {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(SessionChange::Prompt { prompt, .. }) = changes.recv().await {
                break prompt;
            }
        }
    })
    .await
    .expect("prompt change")
}

#[tokio::test]
async fn a_command_runs_only_while_the_shell_is_at_its_prompt() {
    let root = tempfile::tempdir().expect("temp cwd");
    let (changes, mut changes_receiver) = mpsc::channel(128);
    let session = LocalSession::spawn(
        Uuid::now_v7(),
        Uuid::now_v7(),
        PathBuf::from("/bin/sh"),
        vec!["-i".to_owned()],
        root.path().canonicalize().expect("canonical cwd"),
        true,
        changes,
    )
    .await
    .expect("interactive shell");
    assert!(next_prompt(&mut changes_receiver).await);

    let subscription = session.subscribe().await.expect("subscribe");
    session
        .input(
            subscription.connection_id,
            Bytes::from_static(b": half typed "),
        )
        .expect("input");
    session
        .run("printf '\\033]7501;state=done:msg=UmFu\\007'".to_owned())
        .await
        .expect("run at the prompt");
    assert_eq!(
        next_status(&mut changes_receiver)
            .await
            .and_then(|status| status.message),
        Some("Ran".to_owned())
    );

    session
        .run("sleep 30".to_owned())
        .await
        .expect("start a program");
    assert!(!next_prompt(&mut changes_receiver).await);
    assert!(session.run("printf no".to_owned()).await.is_err());
    session.close().await.expect("close");
    drop(session);
}

#[tokio::test]
async fn a_result_stays_after_its_program_ends() {
    let (session, mut changes, _root) = shell(
        "set -m; /bin/sh -c \"printf '\\033]7501;state=done:msg=QnVpbHQ=\\007'\"; exec /bin/cat",
    )
    .await;
    assert_eq!(
        next_status(&mut changes)
            .await
            .map(|status| (status.state, status.message)),
        Some((ProgramState::Done, Some("Built".to_owned())))
    );
    tokio::time::sleep(Duration::from_millis(2500)).await;
    while let Ok(change) = changes.try_recv() {
        assert!(!matches!(change, SessionChange::Status { .. }));
    }
    session.close().await.expect("close");
}

#[tokio::test]
async fn directory_tracking_follows_cd_without_shell_integration() {
    let (session, mut changes, root) = shell("mkdir nested; cd nested; exec /bin/cat").await;
    let expected = root.path().canonicalize().unwrap().join("nested");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(SessionChange::Cwd { path, .. }) = changes.recv().await
                && path == expected
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let subscription = session.subscribe().await.unwrap();
    let cut = session
        .checkpoint(subscription.connection_id)
        .await
        .unwrap();
    assert_eq!(
        cut.checkpoint.metadata.unwrap().directory.as_deref(),
        expected.to_str()
    );
    session.close().await.unwrap();
}

#[tokio::test]
async fn metadata_notifications_follow_changes_not_idle_time() {
    let (session, _changes, _root) =
        shell(r#"stty -echo; while IFS= read -r title; do printf '\033]0;%s\007' "$title"; done"#)
            .await;
    let subscription = session.subscribe().await.unwrap();
    let mut output = session.output();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    while output.try_recv().is_ok() {}
    assert!(
        tokio::time::timeout(Duration::from_millis(150), output.recv())
            .await
            .is_err()
    );
    session
        .input(
            subscription.connection_id,
            Bytes::from_static(b"Changed title\n"),
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(output.recv().await.unwrap(), PublishedFrame::Metadata(_)) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        session
            .checkpoint(subscription.connection_id)
            .await
            .unwrap()
            .checkpoint
            .metadata
            .unwrap()
            .title
            .as_deref(),
        Some("Changed title")
    );
    session.close().await.unwrap();
}

#[tokio::test]
async fn input_completion_waits_for_the_entire_pty_write() {
    let (session, _changes, root) = shell(
        "stty raw -echo; while [ ! -f ready ]; do sleep 0.01; done; printf READY; sleep 0.2; dd bs=1 count=65536 of=received 2>/dev/null; exec /bin/cat",
    )
    .await;
    let mut subscription = session.subscribe().await.unwrap();
    std::fs::write(root.path().join("ready"), b"").unwrap();
    wait_text(&mut subscription, b"READY").await;
    let bytes = Bytes::from(vec![b'x'; 65_536]);
    let completion = session.write_input(subscription.connection_id, bytes.clone());
    tokio::pin!(completion);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut completion)
            .await
            .is_err()
    );
    completion.await.unwrap();
    drop(subscription);
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::metadata(root.path().join("received")).map_or(0, |meta| meta.len()) != 65_536
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(root.path().join("received")).unwrap(), bytes);
    session.close().await.unwrap();
}

#[tokio::test]
async fn input_stays_whole_and_in_order_when_its_caller_stops_waiting() {
    let (session, _changes, root) = shell(
        "stty raw -echo; while [ ! -f ready ]; do sleep 0.01; done; printf READY; sleep 0.3; dd bs=1 count=65540 of=received 2>/dev/null; exec /bin/cat",
    )
    .await;
    let mut subscription = session.subscribe().await.unwrap();
    std::fs::write(root.path().join("ready"), b"").unwrap();
    wait_text(&mut subscription, b"READY").await;
    let first = Bytes::from(vec![b'x'; 65_536]);
    let abandoned = session.write_input(subscription.connection_id, first.clone());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), abandoned)
            .await
            .is_err()
    );
    session
        .write_input(subscription.connection_id, Bytes::from_static(b"tail"))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::metadata(root.path().join("received")).map_or(0, |meta| meta.len()) != 65_540
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut expected = first.to_vec();
    expected.extend_from_slice(b"tail");
    assert_eq!(
        std::fs::read(root.path().join("received")).unwrap(),
        expected
    );
    drop(subscription);
    session.close().await.unwrap();
}

#[tokio::test]
async fn closing_a_view_rejects_its_pending_input_completion() {
    let (session, _changes, root) =
        shell("stty raw -echo; while [ ! -f ready ]; do sleep 0.01; done; printf READY; sleep 30")
            .await;
    let mut subscription = session.subscribe().await.unwrap();
    std::fs::write(root.path().join("ready"), b"").unwrap();
    wait_text(&mut subscription, b"READY").await;
    let completion =
        session.write_input(subscription.connection_id, Bytes::from(vec![b'x'; 65_536]));
    tokio::pin!(completion);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut completion)
            .await
            .is_err()
    );
    drop(subscription);
    assert!(completion.await.is_err());
    session.close().await.unwrap();
}

#[tokio::test]
async fn input_admission_rejects_a_detached_view() {
    let (session, _changes, _root) = shell("exec /bin/cat").await;
    let subscription = session.subscribe().await.unwrap();
    let id = subscription.connection_id;
    session.unsubscribe(id);
    assert!(
        session
            .write_input(id, Bytes::from_static(b"rejected"))
            .await
            .is_err()
    );
    session.close().await.unwrap();
}

#[tokio::test]
async fn title_delivery_retries_after_the_change_channel_is_full() {
    let root = tempfile::tempdir().unwrap();
    let (changes, mut receiver) = mpsc::channel(1);
    changes
        .send(SessionChange::Bell { id: Uuid::now_v7() })
        .await
        .unwrap();
    let local = LocalSession::spawn(
        Uuid::now_v7(),
        Uuid::now_v7(),
        "/bin/sh".into(),
        vec![
            "-c".into(),
            r"printf '\033]0;Final title\007'; exec /bin/cat".into(),
        ],
        root.path().to_owned(),
        true,
        changes,
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    receiver.recv().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(SessionChange::Title { title, .. }) = receiver.recv().await {
                assert_eq!(title, "Final title");
                break;
            }
        }
    })
    .await
    .unwrap();
    local.close().await.unwrap();
    drop(local);
}
