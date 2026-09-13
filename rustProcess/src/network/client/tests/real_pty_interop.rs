use super::interop::{Fixture, execute, ready, signed_in, validate_event};
use super::*;
use crate::network::TerminalControl;
use crate::terminal::{
    Checkpoint, ControlFrame, LocalSession, RemoteTerminal, Subscription, TerminalHistoryPolicy,
    TerminalSize, validate_terminal_checkpoint,
};
use bytes::Bytes;
use ghostty_vt::{CheckpointLimits, SemanticCheckpoint, Terminal, TerminalPolicy};

struct ShellGuard(LocalSession);
impl Drop for ShellGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct View {
    subscription: Subscription,
    renderer: Terminal,
    next: u64,
    received: Vec<u8>,
    resizes: usize,
    pending_resize: Option<(u16, u16, u64)>,
}

impl View {
    fn new(subscription: Subscription) -> Self {
        validate_terminal_checkpoint(&subscription.checkpoint, TerminalHistoryPolicy::default())
            .unwrap();
        let mut renderer = Terminal::new(
            subscription.checkpoint.cols(),
            subscription.checkpoint.rows(),
            TerminalPolicy::default(),
        )
        .unwrap();
        renderer
            .restore_semantic_checkpoint(
                &SemanticCheckpoint::from(subscription.checkpoint.semantic_checkpoint.clone()),
                CheckpointLimits::default(),
            )
            .unwrap();
        let next = subscription.next_sequence;
        Self {
            subscription,
            renderer,
            next,
            received: Vec::new(),
            resizes: 0,
            pending_resize: None,
        }
    }

    fn apply_resize(&mut self) {
        if let Some((rows, cols, at)) = self.pending_resize {
            assert!(
                at >= self.next,
                "resize arrived behind already rendered output"
            );
            if at == self.next {
                self.renderer.resize(cols, rows, 0, 0).unwrap();
                self.pending_resize = None;
            }
        }
    }

    #[expect(
        clippy::future_not_send,
        reason = "Ghostty renderers are thread-affine and stay on the current-thread test runtime"
    )]
    async fn until(&mut self, marker: &str) {
        let outcome=tokio::time::timeout(Duration::from_secs(10),async {
            loop {
                tokio::select! {
                    biased;
                    control=self.subscription.control.recv()=>match control.expect("view control stream ended before output") {
                        ControlFrame::Resize {rows,cols,at_sequence}=>{
                            self.resizes+=1;
                            assert!(self.pending_resize.is_none());
                            self.pending_resize=Some((rows,cols,at_sequence));
                            self.apply_resize();
                        }
                        ControlFrame::Closed {reason,..}=>panic!("view closed before marker: {reason}"),
                    },
                    data=self.subscription.data.recv()=>{
                        let frame=data.expect("view data stream ended before output");
                        assert_eq!(frame.sequence,self.next,"raw output skipped or replayed");
                        self.apply_resize();
                        self.renderer.write(&frame.bytes).unwrap();
                        self.next+=1;
                        self.received.extend_from_slice(&frame.bytes);
                        if self.received.windows(marker.len()).any(|bytes|bytes==marker.as_bytes()){break;}
                    }
                }
            }
        }).await;
        assert!(
            outcome.is_ok(),
            "missing output marker {marker}; received {:?}",
            String::from_utf8_lossy(&self.received)
        );
        assert!(
            self.renderer
                .format_finite_cli_replay()
                .unwrap()
                .windows(marker.len())
                .any(|bytes| bytes == marker.as_bytes()),
            "marker was not rendered by real Ghostty"
        );
    }
}

fn assert_rendered(checkpoint: &Checkpoint, marker: &str) {
    validate_terminal_checkpoint(checkpoint, TerminalHistoryPolicy::default()).unwrap();
    let mut renderer = Terminal::new(
        checkpoint.cols(),
        checkpoint.rows(),
        TerminalPolicy::default(),
    )
    .unwrap();
    renderer
        .restore_semantic_checkpoint(
            &SemanticCheckpoint::from(checkpoint.semantic_checkpoint.clone()),
            CheckpointLimits::default(),
        )
        .unwrap();
    assert!(
        renderer
            .format_finite_cli_replay()
            .unwrap()
            .windows(marker.len())
            .any(|bytes| bytes == marker.as_bytes())
    );
}

fn collect_events(network: &Network) -> (CancellationToken, tokio::task::JoinHandle<Vec<String>>) {
    let mut events = network.events();
    let done = CancellationToken::new();
    let cancel = done.clone();
    let task = tokio::spawn(async move {
        let mut kinds = Vec::new();
        loop {
            tokio::select! {
                biased;
                event=events.recv()=>match event {
                    Ok(event)=>{validate_event(&event.event);kinds.push(event.event["type"].as_str().unwrap().to_owned());}
                    Err(broadcast::error::RecvError::Closed)=>break,
                    Err(broadcast::error::RecvError::Lagged(_))=>panic!("network event collector lagged"),
                },
                ()=cancel.cancelled()=>break,
            }
        }
        kinds
    });
    (done, task)
}

async fn wait_closed(terminal: &RemoteTerminal) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !terminal.is_closed() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

fn process_exists(pid: &str) -> bool {
    assert!(!pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()));
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

async fn approve_second(owner: &Network, second: &Network) {
    let link = execute(second, "devices.link.startSelf", Value::Null).await;
    let code = link
        .events
        .iter()
        .find(|event| event["type"] == "devices.link.selfPending")
        .unwrap()["userCode"]
        .clone();
    execute(owner, "devices.link.approve", json!({"userCode":code})).await;
    second.poll_link().await.unwrap();
    assert!(second.identity().unwrap().enrolled);
}

async fn mission_without_terminal_access(
    owner: &Network,
    friend: &Network,
    id: Uuid,
    incarnation: Uuid,
) -> Uuid {
    let room = Uuid::now_v7();
    execute(
        owner,
        "room.create",
        json!({"requestId":room,"name":"Isolated E2E","slug":format!("e2e-{room}")}),
    )
    .await;
    execute(
        owner,
        "room.open",
        json!({"requestId":Uuid::now_v7(),"roomId":room}),
    )
    .await;
    let invitation = Uuid::now_v7();
    execute(
        owner,
        "room.invite",
        json!({"requestId":invitation,"roomId":room,"userId":friend.identity().unwrap().user_id}),
    )
    .await;
    execute(friend, "room.list", Value::Null).await;
    execute(
        friend,
        "room.invitation.accept",
        json!({"requestId":Uuid::now_v7(),"invitationId":invitation}),
    )
    .await;
    execute(owner,"session.attachMission",json!({"requestId":Uuid::now_v7(),"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"roomId":room})).await;
    execute(
        friend,
        "room.open",
        json!({"requestId":Uuid::now_v7(),"roomId":room}),
    )
    .await;
    assert!(
        friend.connect_remote(id).await.is_err(),
        "Mission membership must not grant terminal access"
    );
    room
}

#[expect(
    clippy::too_many_lines,
    reason = "one real PTY, encrypted relay and native-renderer lifecycle journey"
)]
#[tokio::test]
#[ignore = "requires KODOSI_NETWORK_TEST_FIXTURE for a fresh disposable loopback backend and pinned native Ghostty"]
async fn real_pty_encrypted_relay_and_renderers() {
    let fixture: Fixture = serde_json::from_slice(
        &std::fs::read(std::env::var("KODOSI_NETWORK_TEST_FIXTURE").unwrap()).unwrap(),
    )
    .unwrap();
    let base = reqwest::Url::parse(&fixture.base_url).unwrap();
    assert!(base.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    }));
    let root = tempfile::tempdir().unwrap();
    let owner = signed_in(&root.path().join("owner"), &base, &fixture.owner_token).await;
    let friend = signed_in(&root.path().join("friend"), &base, &fixture.friend_token).await;
    let second = signed_in(&root.path().join("second"), &base, &fixture.owner_token).await;
    let (owner_done, owner_events) = collect_events(&owner);
    let (friend_done, friend_events) = collect_events(&friend);
    let (second_done, second_events) = collect_events(&second);
    approve_second(&owner, &second).await;
    let owner_profile: Value = owner
        .inner
        .http
        .bearer(Method::GET, "api/me", &fixture.owner_token, None)
        .await
        .unwrap();
    let friend_profile: Value = friend
        .inner
        .http
        .bearer(Method::GET, "api/me", &fixture.friend_token, None)
        .await
        .unwrap();
    execute(
        &owner,
        "friends.request.send",
        json!({"requestId":Uuid::now_v7(),"username":friend_profile["handle"]}),
    )
    .await;
    execute(
        &friend,
        "friends.request.accept",
        json!({"username":owner_profile["handle"]}),
    )
    .await;
    execute(&owner, "devices.refresh", Value::Null).await;
    execute(&owner, "auth.refresh", Value::Null).await;

    let directory = root.path().join("terminal");
    std::fs::create_dir(&directory).unwrap();
    let id = Uuid::now_v7();
    let incarnation = Uuid::now_v7();
    let (changes, mut observed_changes) = mpsc::channel(128);
    let local=LocalSession::spawn(id,incarnation,"/bin/sh".into(),vec!["-c".into(),"umask 077; printf '%s\\n' \"$$\" > shell.pid; stty -echo; PS1= PS2= ENV=/dev/null; export PS1 PS2 ENV; exec /bin/sh -s".into()],directory.canonicalize().unwrap(),true,changes.clone()).await.unwrap();
    let _shell_guard = ShellGuard(local.clone());
    let observer = local.subscribe().await.unwrap();
    owner
        .publish(
            LocalPublication {
                session_id: id,
                incarnation_id: incarnation,
                name: "Real PTY E2E".into(),
                room_id: None,
                shared_with: BTreeSet::new(),
            },
            local.host_requests.clone(),
            local.output(),
        )
        .await
        .unwrap();
    ready(&owner, id).await;
    let rejected = owner
        .set_shares(
            id,
            incarnation,
            BTreeSet::from([Uuid::now_v7().to_string()]),
        )
        .await;
    assert!(matches!(rejected, Err(Error::Backend { status: 403, .. })));
    let publication = Arc::clone(&owner.inner.publications.lock().await[&id]);
    assert!(publication.pending_shares.lock().await.is_none());
    assert!(publication.info.read().await.shared_with.is_empty());
    drop(publication);
    ready(&owner, id).await;
    let pid = std::fs::read_to_string(directory.join("shell.pid")).unwrap();
    let pid = pid.trim();
    assert!(process_exists(pid));
    assert!(
        friend.connect_remote(id).await.is_err(),
        "friendship alone must not grant access"
    );
    let mission = mission_without_terminal_access(&owner, &friend, id, incarnation).await;

    let own_remote =
        RemoteTerminal::spawn(second.connect_remote(id).await.unwrap(), changes.clone());
    let mut own_view = View::new(own_remote.subscribe().await.unwrap());
    own_remote
        .admit_input(
            own_view.subscription.connection_id,
            Bytes::from_static(b"printf 'OWN_%s\\n' 'DEVICE_OK'\n"),
        )
        .await
        .unwrap();
    own_view.until("OWN_DEVICE_OK").await;
    own_remote.disconnect();
    drop(own_view);
    drop(own_remote);

    execute(&owner,"session.share",json!({"requestId":Uuid::now_v7(),"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"userIds":[friend.identity().unwrap().user_id]})).await;
    ready(&owner, id).await;
    let remote = RemoteTerminal::spawn(friend.connect_remote(id).await.unwrap(), changes.clone());
    let mut first = View::new(remote.subscribe().await.unwrap());
    remote
        .admit_input(
            first.subscription.connection_id,
            Bytes::from_static(b"printf 'REAL_%s\\n' 'PTY_ONE'\n"),
        )
        .await
        .unwrap();
    first.until("REAL_PTY_ONE").await;
    let current = remote
        .checkpoint(first.subscription.connection_id)
        .await
        .unwrap();
    assert_rendered(&current.checkpoint, "REAL_PTY_ONE");
    let cut_before_late = first.next;
    remote.input(first.subscription.connection_id,Bytes::from_static(b"for n in 1 2 3 4 5 6 7 8 9 10; do printf 'LIVE_%s\\n' \"$n\"; sleep 0.02; done; printf 'LATE_%s\\n' 'CAPTURE_DONE'\n")).unwrap();
    let (late, ()) = tokio::join!(remote.subscribe(), first.until("LATE_CAPTURE_DONE"));
    let mut late = View::new(late.unwrap());
    assert!(first.next > cut_before_late);
    assert_eq!(
        first.resizes, 0,
        "late view must not reset or resize existing renderer"
    );
    assert!(first.subscription.control.try_recv().is_err());
    remote
        .input(
            late.subscription.connection_id,
            Bytes::from_static(b"printf 'AFTER_%s\\n' 'LATE_JOIN'\n"),
        )
        .unwrap();
    tokio::join!(
        first.until("AFTER_LATE_JOIN"),
        late.until("AFTER_LATE_JOIN")
    );
    assert_eq!(first.next, late.next);

    remote
        .control(
            Some(first.subscription.connection_id),
            TerminalControl::Resize {
                request_id: Uuid::now_v7().to_string(),
                rows: 18,
                cols: 70,
                width_pixels: 0,
                height_pixels: 0,
                cell_width_pixels: 0,
                cell_height_pixels: 0,
                claim: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        local
            .checkpoint(observer.connection_id)
            .await
            .unwrap()
            .checkpoint
            .size(),
        TerminalSize::new(18, 70).unwrap()
    );
    remote
        .input(
            first.subscription.connection_id,
            Bytes::from_static(b"printf 'GEOMETRY='; stty size; printf 'RESIZE_%s\\n' 'DONE'\n"),
        )
        .unwrap();
    tokio::join!(first.until("RESIZE_DONE"), late.until("RESIZE_DONE"));
    assert!(
        first
            .received
            .windows(b"GEOMETRY=18 70".len())
            .any(|bytes| bytes == b"GEOMETRY=18 70")
    );
    assert_eq!(first.renderer.state().unwrap().rows, 18);
    assert_eq!(late.renderer.state().unwrap().cols, 70);
    assert_eq!(first.resizes, 1);
    assert_eq!(late.resizes, 1);
    assert_rendered(
        &remote
            .checkpoint(first.subscription.connection_id)
            .await
            .unwrap()
            .checkpoint,
        "RESIZE_DONE",
    );

    execute(&owner,"session.share",json!({"requestId":Uuid::now_v7(),"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"userIds":[]})).await;
    wait_closed(&remote).await;
    assert!(
        remote
            .input(
                first.subscription.connection_id,
                Bytes::from_static(b"printf 'revoked'\n")
            )
            .is_err()
    );
    assert!(remote.control(None, TerminalControl::Stop).await.is_err());
    assert!(process_exists(pid));
    assert!(!local.is_closed());
    assert!(friend.connect_remote(id).await.is_err());
    drop(first);
    drop(late);
    drop(remote);

    execute(&owner,"session.share",json!({"requestId":Uuid::now_v7(),"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"userIds":[friend.identity().unwrap().user_id]})).await;
    ready(&owner, id).await;
    let stopper = RemoteTerminal::spawn(friend.connect_remote(id).await.unwrap(), changes);
    let final_view = stopper.subscribe().await.unwrap();
    validate_terminal_checkpoint(&final_view.checkpoint, TerminalHistoryPolicy::default()).unwrap();
    stopper
        .control(Some(final_view.connection_id), TerminalControl::Stop)
        .await
        .unwrap();
    local.stop().await.unwrap();
    assert!(
        !process_exists(pid),
        "remote Stop must reap the real shell process"
    );
    assert!(local.is_closed());
    drop(local);
    tokio::time::timeout(Duration::from_secs(5),async {loop {if matches!(observed_changes.recv().await,Some(crate::terminal::SessionChange::Ended {id:ended,..}) if ended==id){break;}}}).await.unwrap();
    owner.unpublish(id).await.unwrap();
    execute(
        &owner,
        "room.delete",
        json!({"requestId":Uuid::now_v7(),"roomId":mission}),
    )
    .await;
    stopper.disconnect();
    drop(stopper);
    security_interop::revoked_device_can_explicitly_reapprove(&owner, &second).await;
    Box::pin(
        security_interop::expired_friend_does_not_block_owner_and_retirement_unpublishes(
            &owner, &friend, &second, &directory,
        ),
    )
    .await;
    owner.shutdown().await;
    friend.shutdown().await;
    second.shutdown().await;
    owner_done.cancel();
    friend_done.cancel();
    second_done.cancel();
    let kinds = owner_events
        .await
        .unwrap()
        .into_iter()
        .chain(friend_events.await.unwrap())
        .chain(second_events.await.unwrap())
        .collect::<Vec<_>>();
    assert!(kinds.iter().any(|kind| kind == "friends.snapshot"));
    assert!(kinds.iter().any(|kind| kind == "devices.list"));
    assert!(kinds.iter().any(|kind| kind == "rooms.snapshot"));
}
