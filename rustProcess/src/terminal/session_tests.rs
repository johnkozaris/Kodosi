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
async fn launch_inherits_environment_without_agent_session_injection() {
    let (session, _changes, root) = shell(
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
    session.stop().await.expect("stop");

    let inherited_id = std::env::var_os("KODOSI_SESSION_ID").unwrap_or_default();
    assert_eq!(
        std::fs::read(root.path().join("session-id")).expect("inherited session marker"),
        inherited_id.as_encoded_bytes(),
        "PTY launch must not manufacture an agent session identity"
    );
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
    session.stop().await.expect("stop");
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
    session.stop().await.expect("stop");
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
    session.stop().await.expect("stop");
}

#[tokio::test]
async fn canceled_host_authorization_cannot_resize_focus_or_stop() {
    let (session, _changes, _root) = shell("exec /bin/cat").await;
    let subscription = session.subscribe().await.expect("subscribe");
    let authorization = CancellationToken::new();
    authorization.cancel();
    let connection_id = Uuid::now_v7();
    for control in [
        TerminalControl::Stop,
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
    session.stop().await.expect("stop");
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
        .send(HostRequest::Bootstrap { request_id, reply })
        .await
        .expect("bootstrap");
    let cut = result.await.expect("bootstrap reply").expect("cut");
    let mut resized = false;
    loop {
        match output.recv().await.expect("ordered frame") {
            PublishedFrame::Checkpoint {
                checkpoint,
                next_sequence,
            } => {
                assert_eq!(checkpoint.size(), size);
                assert_eq!(next_sequence, cut.next_sequence);
                resized = true;
            }
            PublishedFrame::BootstrapBarrier {
                request_id: received,
            } if received == request_id => {
                assert!(resized);
                break;
            }
            PublishedFrame::Raw { .. } | PublishedFrame::MetadataChanged => {}
            _ => panic!("unexpected publication frame"),
        }
    }
    session.stop().await.expect("stop");
}

#[tokio::test]
async fn simultaneous_stop_waiters_confirm_reaping_even_with_full_admission_queue() {
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
    let stop_one = session.stop();
    let stop_two = session.stop();
    assert!(session.is_closed());
    assert!(
        session
            .input(subscription.connection_id, Bytes::from_static(b"late"))
            .is_err()
    );
    let (first, second) = tokio::join!(stop_one, stop_two);
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
        viewers: HashMap::new(),
        working_directory: None,
        title: None,
        published_title: None,
        metadata_dirty: false,
        next_program_check: tokio::time::Instant::now(),
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
    session.stop().await.unwrap();
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
            if matches!(
                output.recv().await.unwrap(),
                PublishedFrame::MetadataChanged
            ) {
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
    session.stop().await.unwrap();
}

#[tokio::test]
async fn input_admission_rejects_a_detached_view() {
    let (session, _changes, _root) = shell("exec /bin/cat").await;
    let subscription = session.subscribe().await.unwrap();
    let id = subscription.connection_id;
    session.unsubscribe(id);
    assert!(
        session
            .admit_input(id, Bytes::from_static(b"rejected"))
            .await
            .is_err()
    );
    session.stop().await.unwrap();
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
    local.stop().await.unwrap();
    drop(local);
}
