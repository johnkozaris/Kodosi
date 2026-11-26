use std::sync::atomic::AtomicBool;

use super::*;
use crate::identity::keys::DeviceKeys;

struct Relay {
    to_peer: mpsc::UnboundedSender<Result<Vec<u8>>>,
    from_peer: mpsc::UnboundedReceiver<Result<Vec<u8>>>,
}

impl Transport for Relay {
    fn send(&mut self, bytes: Vec<u8>) -> impl Future<Output = Result<()>> + Send {
        std::future::ready(self.to_peer.send(Ok(bytes)).map_err(|_| Error::Closed))
    }

    async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        self.from_peer.recv().await.transpose()
    }
}

type Service = mpsc::UnboundedSender<Result<Vec<u8>>>;

fn routes() -> ((Route, Paths), (Route, Paths), Service) {
    let (to_host, from_viewer) = mpsc::unbounded_channel();
    let (to_viewer, from_host) = mpsc::unbounded_channel();
    let service = to_viewer.clone();
    let viewer = Route::new(
        Relay {
            to_peer: to_host,
            from_peer: from_host,
        },
        Role::Viewer,
    );
    let host = Route::new(
        Relay {
            to_peer: to_viewer,
            from_peer: from_viewer,
        },
        Role::Host,
    );
    (viewer, host, service)
}

struct Wire {
    cut: CancellationToken,
    silent: Arc<AtomicBool>,
    near_abandoned: CancellationToken,
    far_abandoned: CancellationToken,
}

fn lanes() -> (Lane, Lane, Wire) {
    let wire = Wire {
        cut: CancellationToken::new(),
        silent: Arc::new(AtomicBool::new(false)),
        near_abandoned: CancellationToken::new(),
        far_abandoned: CancellationToken::new(),
    };
    let (near_out, near_sent) = mpsc::unbounded_channel();
    let (far_out, far_sent) = mpsc::unbounded_channel();
    let (near_in, near_heard) = mpsc::unbounded_channel();
    let (far_in, far_heard) = mpsc::unbounded_channel();
    for (sent, heard) in [(near_sent, far_in), (far_sent, near_in)] {
        tokio::spawn(carry(
            sent,
            heard,
            wire.cut.clone(),
            Arc::clone(&wire.silent),
        ));
    }
    (
        Lane::memory(near_out, near_heard, wire.near_abandoned.clone()),
        Lane::memory(far_out, far_heard, wire.far_abandoned.clone()),
        wire,
    )
}

async fn carry(
    mut sent: mpsc::UnboundedReceiver<Vec<u8>>,
    heard: mpsc::UnboundedSender<Vec<u8>>,
    cut: CancellationToken,
    silent: Arc<AtomicBool>,
) {
    loop {
        let message = tokio::select! {
            () = cut.cancelled() => return,
            message = sent.recv() => message,
        };
        let Some(message) = message else {
            if silent.load(Ordering::Acquire) {
                cut.cancelled().await;
            }
            return;
        };
        if !silent.load(Ordering::Acquire) && heard.send(message).is_err() {
            return;
        }
    }
}

async fn exchange(
    mut route: Route,
    name: char,
    count: usize,
    mut allowed: watch::Receiver<usize>,
    seen: Arc<AtomicUsize>,
) -> (Vec<String>, Route) {
    let (mut sent, mut received) = (0, Vec::new());
    while sent < count || received.len() < count {
        let may_send = sent < count && sent < *allowed.borrow();
        tokio::select! {
            message = route.receive(), if received.len() < count => {
                let message = message.unwrap().unwrap();
                received.push(String::from_utf8(message).unwrap());
                seen.fetch_add(1, Ordering::AcqRel);
            }
            () = std::future::ready(()), if may_send => {
                route.send(format!("{name}{sent}").into_bytes()).await.unwrap();
                sent += 1;
            }
            changed = allowed.changed(), if !may_send && sent < count => changed.unwrap(),
        }
    }
    (received, route)
}

async fn until_seen(seen: &[&Arc<AtomicUsize>], count: usize) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while seen.iter().any(|seen| seen.load(Ordering::Acquire) < count) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn each_message_arrives_once_and_in_order_while_the_route_moves_to_a_lane_and_back() {
    let ((viewer, mut viewer_paths), (host, host_paths), _service) = routes();
    let (near, far, wire) = lanes();
    let count = 400;
    let (allow, allowed) = watch::channel(100);
    let (viewer_seen, host_seen) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let viewing = tokio::spawn(exchange(
        viewer,
        'v',
        count,
        allowed.clone(),
        Arc::clone(&viewer_seen),
    ));
    let hosting = tokio::spawn(exchange(host, 'h', count, allowed, Arc::clone(&host_seen)));
    until_seen(&[&viewer_seen, &host_seen], 100).await;

    assert!(host_paths.attach(far));
    assert!(viewer_paths.attach(near));
    allow.send(200).unwrap();
    assert!(viewer_paths.went_direct().await);
    assert_eq!(*viewer_paths.path.borrow(), Path::Direct);
    until_seen(&[&viewer_seen, &host_seen], 200).await;

    allow.send(count).unwrap();
    until_seen(&[&viewer_seen, &host_seen], 250).await;
    wire.cut.cancel();
    let ((at_viewer, _viewer), (at_host, _host)) = (viewing.await.unwrap(), hosting.await.unwrap());
    assert_eq!(
        at_viewer,
        (0..count)
            .map(|index| format!("h{index}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        at_host,
        (0..count)
            .map(|index| format!("v{index}"))
            .collect::<Vec<_>>()
    );
    assert!(viewer_paths.back_on_relay().await);
}

#[tokio::test(start_paused = true)]
async fn a_silent_lane_ends_its_connection_and_what_it_swallowed_arrives_over_the_relay() {
    let ((mut viewer, mut viewer_paths), (mut host, host_paths), _service) = routes();
    let (near, far, wire) = lanes();
    host_paths.attach(far);
    viewer_paths.attach(near);
    assert!(viewer_paths.went_direct().await);
    assert_eq!(*viewer_paths.path.borrow(), Path::Direct);

    wire.silent.store(true, Ordering::Release);
    viewer.send(b"typed".to_vec()).await.unwrap();
    host.send(b"echoed".to_vec()).await.unwrap();
    let heard = tokio::time::timeout(Duration::from_secs(30), host.receive()).await;
    assert_eq!(heard.unwrap().unwrap().unwrap(), b"typed");
    let heard = tokio::time::timeout(Duration::from_secs(30), viewer.receive()).await;
    assert_eq!(heard.unwrap().unwrap().unwrap(), b"echoed");
    assert_eq!(*viewer_paths.path.borrow(), Path::Relay);
    assert!(wire.near_abandoned.is_cancelled() && wire.far_abandoned.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_view_ends_the_connection_of_a_lane_that_its_host_found_silent() {
    let ((mut viewer, mut viewer_paths), (mut host, host_paths), _service) = routes();
    let (near, far, wire) = lanes();
    host_paths.attach(far);
    viewer_paths.attach(near);
    assert!(viewer_paths.went_direct().await);

    wire.silent.store(true, Ordering::Release);
    host.send(b"output".to_vec()).await.unwrap();
    let heard = tokio::time::timeout(Duration::from_secs(30), viewer.receive()).await;
    assert_eq!(heard.unwrap().unwrap().unwrap(), b"output");
    assert!(wire.far_abandoned.is_cancelled());
    assert!(wire.near_abandoned.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_route_is_late_only_while_its_messages_wait_for_an_answer() {
    let ((mut viewer, mut viewer_paths), (mut host, host_paths), _service) = routes();
    let (near, far, wire) = lanes();
    host_paths.attach(far);
    viewer_paths.attach(near);
    assert!(viewer_paths.went_direct().await);
    let mut late = viewer_paths.late();

    wire.silent.store(true, Ordering::Release);
    viewer.send(b"typed".to_vec()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!*late.borrow());
    tokio::time::timeout(Duration::from_secs(2), late.wait_for(|late| *late))
        .await
        .unwrap()
        .unwrap();
    let heard = tokio::time::timeout(Duration::from_secs(30), host.receive()).await;
    assert_eq!(heard.unwrap().unwrap().unwrap(), b"typed");
    tokio::time::timeout(Duration::from_secs(5), late.wait_for(|late| !*late))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a_route_that_ends_on_a_lane_delivers_its_last_messages_first() {
    let ((mut viewer, mut viewer_paths), (mut host, host_paths), service) = routes();
    drop(service);
    let (near, far, _wire) = lanes();
    host_paths.attach(far);
    viewer_paths.attach(near);
    assert!(viewer_paths.went_direct().await);

    host.send(b"last".to_vec()).await.unwrap();
    drop(host);
    let heard = tokio::time::timeout(Duration::from_secs(10), viewer.receive()).await;
    assert_eq!(heard.unwrap().unwrap().unwrap(), b"last");
    let ended = tokio::time::timeout(Duration::from_secs(10), viewer.receive()).await;
    assert!(!matches!(ended.unwrap(), Ok(Some(_))));
}

#[tokio::test(start_paused = true)]
async fn a_route_that_ends_on_a_lane_stops_when_nothing_answers_its_last_messages() {
    let (to_host, mut host_hears) = mpsc::unbounded_channel();
    let (_host_says, from_host) = mpsc::unbounded_channel();
    let (mut viewer, mut paths) = Route::new(
        Relay {
            to_peer: to_host,
            from_peer: from_host,
        },
        Role::Viewer,
    );
    let (near, mut far, wire) = lanes();
    paths.attach(near);
    far.reader.recv().await.unwrap();
    far.write(&numbered(RESUME, 0)).await.unwrap();
    assert!(paths.went_direct().await);

    wire.silent.store(true, Ordering::Release);
    viewer.send(b"last".to_vec()).await.unwrap();
    drop(viewer);
    let stopped = tokio::time::timeout(Duration::from_secs(60), async {
        while host_hears.recv().await.is_some() {}
    })
    .await;
    assert!(stopped.is_ok());
}

#[tokio::test]
async fn the_service_ends_the_route_by_its_relay_pipe_while_the_messages_go_straight() {
    let ((mut viewer, mut viewer_paths), (_host, host_paths), service) = routes();
    let (near, far, _wire) = lanes();
    host_paths.attach(far);
    viewer_paths.attach(near);
    assert!(viewer_paths.went_direct().await);
    service
        .send(Err(Error::Backend {
            status: 403,
            message: text!("This terminal is not shared with you."),
        }))
        .unwrap();
    assert!(matches!(
        viewer.receive().await,
        Err(Error::Backend { status: 403, .. })
    ));
}

#[tokio::test]
async fn a_host_moves_to_a_lane_only_when_its_viewer_does() {
    let ((mut viewer, viewer_paths), (mut host, host_paths), _service) = routes();
    let (_near, far, _wire) = lanes();
    host_paths.attach(far);
    host.send(b"output".to_vec()).await.unwrap();
    assert_eq!(viewer.receive().await.unwrap().unwrap(), b"output");
    assert_eq!(
        (*host_paths.path.borrow(), *viewer_paths.path.borrow()),
        (Path::Relay, Path::Relay)
    );
}

#[tokio::test]
async fn a_channel_keeps_its_session_while_its_route_moves() {
    use super::super::channel::{Channel, tests::channel_keys};
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let ((viewer, mut viewer_paths), (host, host_paths), _service) = routes();
    let (hosting, viewing) = (channel_keys(&host_keys), channel_keys(&viewer_keys));
    let (accepted, connected) = tokio::join!(
        Channel::accept(host, &hosting),
        Channel::connect(viewer, &viewing, "host-device", host_keys.signing_public()),
    );
    let (mut hosted, mut view) = (accepted.unwrap(), connected.unwrap());
    hosted.send(&Frame::Ack { received: 1 }).await.unwrap();
    assert!(matches!(
        view.receive().await,
        Ok(Some((Frame::Ack { received: 1 }, _)))
    ));

    let (near, far, _wire) = lanes();
    host_paths.attach(far);
    viewer_paths.attach(near);
    assert!(viewer_paths.went_direct().await);
    hosted.send(&Frame::Ack { received: 2 }).await.unwrap();
    assert!(matches!(
        view.receive().await,
        Ok(Some((Frame::Ack { received: 2 }, _)))
    ));
    view.send(&Frame::Refresh).await.unwrap();
    assert!(matches!(
        hosted.receive().await,
        Ok(Some((Frame::Refresh, _)))
    ));
}

#[cfg(feature = "host")]
#[tokio::test]
async fn two_finders_take_a_view_straight_and_its_channel_goes_on_over_the_lane() {
    use super::super::channel::{Channel, tests::channel_keys};
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (hosting, viewing) = (
        Arc::new(channel_keys(&host_keys)),
        Arc::new(channel_keys(&viewer_keys)),
    );
    let ((viewer, mut viewer_paths), (host, host_paths), _service) = routes();
    let (accepted, connected) = tokio::join!(
        Channel::accept(host, &hosting),
        Channel::connect(viewer, &viewing, "host-device", host_keys.signing_public()),
    );
    let (mut hosted, mut view) = (accepted.unwrap(), connected.unwrap());
    let pipe = Uuid::now_v7();
    let mut viewer_finder = Finder::viewer(
        Some(Peers::on_loopback(viewing)),
        viewer_paths.clone(),
        (host_keys.signing_public().to_vec(), pipe, true),
        Arc::new(Link::default()),
    );
    let mut host_finder = Finder::host(
        Some(Peers::on_loopback(hosting)),
        host_paths,
        (viewer_keys.signing_public().to_vec(), pipe, true),
    );
    let straight = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            tokio::select! {
                () = async { viewer_paths.path.wait_for(|path| *path != Path::Relay).await.unwrap(); } => return,
                Some(frame) = viewer_finder.next() => {
                    view.send(&frame).await.unwrap();
                }
                Some(frame) = host_finder.next() => {
                    hosted.send(&frame).await.unwrap();
                }
                frame = hosted.receive() => host_finder.heard(frame.unwrap().unwrap().0),
                frame = view.receive() => viewer_finder.heard(frame.unwrap().unwrap().0),
            }
        }
    })
    .await;
    assert!(straight.is_ok());
    assert_eq!(*viewer_paths.path.borrow(), Path::Direct);

    hosted.send(&Frame::Ack { received: 7 }).await.unwrap();
    assert!(matches!(
        view.receive().await,
        Ok(Some((Frame::Ack { received: 7 }, _)))
    ));
    view.send(&Frame::Refresh).await.unwrap();
    assert!(matches!(
        hosted.receive().await,
        Ok(Some((Frame::Refresh, _)))
    ));
}

#[cfg(feature = "host")]
#[tokio::test]
async fn the_devices_of_two_people_meet_only_through_the_relay_and_trade_no_addresses() {
    use super::super::{
        channel::{Channel, tests::channel_keys},
        peers::tests::{granted, listening, relay_for_tests},
    };
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (hosting, viewing) = (
        Arc::new(channel_keys(&host_keys)),
        Arc::new(channel_keys(&viewer_keys)),
    );
    let ((viewer, mut viewer_paths), (host, host_paths), _service) = routes();
    let (accepted, connected) = tokio::join!(
        Channel::accept(host, &hosting),
        Channel::connect(viewer, &viewing, "host-device", host_keys.signing_public()),
    );
    let (mut hosted, mut view) = (accepted.unwrap(), connected.unwrap());
    let pipe = Uuid::now_v7();
    let (host_peers, viewer_peers) = (Peers::on_loopback(hosting), Peers::on_loopback(viewing));
    granted(&host_peers, &viewer_peers, relay_for_tests().await, pipe);
    let host_address = listening(&host_peers).await;
    let mut viewer_finder = Finder::viewer(
        Some(viewer_peers),
        viewer_paths.clone(),
        (host_keys.signing_public().to_vec(), pipe, false),
        Arc::new(Link::default()),
    );
    let mut host_finder = Finder::host(
        Some(host_peers),
        host_paths,
        (viewer_keys.signing_public().to_vec(), pipe, false),
    );
    let met = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            tokio::select! {
                () = async { viewer_paths.path.wait_for(|path| *path != Path::Relay).await.unwrap(); } => return,
                Some(frame) = viewer_finder.next() => {
                    assert!(matches!(&frame, Frame::PathOffer { addresses } if addresses.is_empty()));
                    view.send(&frame).await.unwrap();
                }
                Some(frame) = host_finder.next() => {
                    assert!(matches!(&frame, Frame::PathAnswer { addresses } if addresses.is_empty()));
                    // A host that answers with its address anyway is still met only through the relay.
                    hosted.send(&Frame::PathAnswer { addresses: vec![host_address] }).await.unwrap();
                }
                frame = hosted.receive() => host_finder.heard(frame.unwrap().unwrap().0),
                frame = view.receive() => viewer_finder.heard(frame.unwrap().unwrap().0),
            }
        }
    })
    .await;
    assert!(met.is_ok());
    assert_eq!(*viewer_paths.path.borrow(), Path::Forwarded);
    hosted.send(&Frame::Ack { received: 3 }).await.unwrap();
    assert!(matches!(
        view.receive().await,
        Ok(Some((Frame::Ack { received: 3 }, _)))
    ));
}
