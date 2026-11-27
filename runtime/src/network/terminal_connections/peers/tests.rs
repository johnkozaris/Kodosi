use super::{super::channel::tests::channel_keys, *};
use crate::identity::keys::DeviceKeys;

fn peers_of(keys: &DeviceKeys) -> Arc<Peers> {
    Peers::new(Arc::new(channel_keys(keys)))
}

#[cfg(feature = "host")]
async fn on_this_computer(peers: &Arc<Peers>) -> Vec<(SocketAddr, bool)> {
    let port = peers
        .bind()
        .await
        .unwrap()
        .socket
        .local_addr()
        .unwrap()
        .port();
    vec![(SocketAddr::from((Ipv4Addr::LOCALHOST, port)), true)]
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_viewer_reaches_the_host_that_expects_it_and_each_route_gets_its_own_lane() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let address = on_this_computer(&host).await;
    for pipe in [Uuid::now_v7(), Uuid::now_v7()] {
        let expected = host.expect(viewer_keys.signing_public(), pipe);
        let (arrived, reached) = tokio::join!(
            expected.meet(&[]),
            viewer.reach_at(host_keys.signing_public(), address.clone(), pipe),
        );
        let (mut arrived, mut reached) = (arrived.unwrap(), reached.unwrap());
        reached.write(pipe.as_bytes()).await.unwrap();
        assert_eq!(arrived.reader.recv().await.unwrap(), pipe.as_bytes());
        arrived.write(b"back").await.unwrap();
        assert_eq!(reached.reader.recv().await.unwrap(), b"back");
    }
    assert_eq!(viewer.dialed.lock().unwrap().len(), 1);
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_host_takes_no_stream_from_a_device_that_no_route_expects() {
    let (host_keys, viewer_keys, stranger_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, stranger) = (peers_of(&host_keys), peers_of(&stranger_keys));
    let pipe = Uuid::now_v7();
    let _expected = host.expect(viewer_keys.signing_public(), pipe);
    let address = on_this_computer(&host).await;
    assert!(
        stranger
            .reach_at(host_keys.signing_public(), address, pipe)
            .await
            .is_none()
    );
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_viewer_leaves_a_device_that_proves_another_key() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let pipe = Uuid::now_v7();
    let _expected = host.expect(viewer_keys.signing_public(), pipe);
    let address = on_this_computer(&host).await;
    let elsewhere = DeviceKeys::generate().unwrap();
    assert!(
        viewer
            .reach_at(elsewhere.signing_public(), address, pipe)
            .await
            .is_none()
    );
    assert!(viewer.dialed.lock().unwrap().is_empty());
}

#[cfg(feature = "host")]
#[tokio::test]
async fn two_devices_of_one_person_go_straight_at_once_and_one_that_leaves_spares_the_other() {
    let (host_keys, laptop_keys, phone_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, laptop, phone) = (
        peers_of(&host_keys),
        peers_of(&laptop_keys),
        peers_of(&phone_keys),
    );
    let address = on_this_computer(&host).await;
    let (laptop_pipe, phone_pipe) = (Uuid::now_v7(), Uuid::now_v7());
    let (laptop_arrived, laptop_reached) = tokio::join!(
        host.expect(laptop_keys.signing_public(), laptop_pipe)
            .meet(&[]),
        laptop.reach_at(host_keys.signing_public(), address.clone(), laptop_pipe),
    );
    let (mut laptop_arrived, mut laptop_reached) =
        (laptop_arrived.unwrap(), laptop_reached.unwrap());
    let (phone_arrived, phone_reached) = tokio::join!(
        host.expect(phone_keys.signing_public(), phone_pipe)
            .meet(&[]),
        phone.reach_at(host_keys.signing_public(), address, phone_pipe),
    );
    let (mut phone_arrived, mut phone_reached) = (phone_arrived.unwrap(), phone_reached.unwrap());
    for (reached, arrived) in [
        (&mut laptop_reached, &mut laptop_arrived),
        (&mut phone_reached, &mut phone_arrived),
    ] {
        reached.write(b"typed").await.unwrap();
        assert_eq!(arrived.reader.recv().await.unwrap(), b"typed");
    }

    drop(laptop_reached);
    drop(laptop);
    let ended = tokio::time::timeout(Duration::from_secs(5), laptop_arrived.reader.recv()).await;
    assert_eq!(ended.unwrap(), None);
    phone_reached.write(b"more").await.unwrap();
    assert_eq!(phone_arrived.reader.recv().await.unwrap(), b"more");
    phone_arrived.write(b"echoed").await.unwrap();
    assert_eq!(phone_reached.reader.recv().await.unwrap(), b"echoed");
}

/// On Apple platforms, sending to a closed local UDP port yields a read error on the
/// connected socket, exercising the same recovery path as an iOS background-closed socket.
#[cfg(all(target_vendor = "apple", feature = "host"))]
#[tokio::test]
async fn a_viewer_whose_socket_fails_goes_straight_again_with_a_new_socket() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let address = on_this_computer(&host).await;
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), first).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address.clone(), first),
    );
    let (_arrived, mut reached) = (arrived.unwrap(), reached.unwrap());

    let socket = viewer.bind().await.unwrap().socket.clone();
    let free = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let refusing = if socket.0.dual {
        SocketAddr::new(IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()), free)
    } else {
        SocketAddr::from((Ipv4Addr::LOCALHOST, free))
    };
    socket.0.io.connect(refusing).await.unwrap();
    // The system answers a datagram to a closed port only within its limit of such answers, which
    // other tests that run at the same time also use.
    let mut ended = None;
    for _ in 0..50 {
        socket.0.io.send(&[0]).await.unwrap();
        if let Ok(end) =
            tokio::time::timeout(Duration::from_millis(100), reached.reader.recv()).await
        {
            ended = Some(end);
            break;
        }
    }
    assert_eq!(ended, Some(None));

    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), second).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address, second),
    );
    let (mut arrived, mut reached) = (arrived.unwrap(), reached.unwrap());
    reached.write(b"typed").await.unwrap();
    assert_eq!(arrived.reader.recv().await.unwrap(), b"typed");
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_paused_viewer_ends_its_straight_lanes_and_a_new_network_gives_it_a_new_socket() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let address = on_this_computer(&host).await;
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), first).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address.clone(), first),
    );
    let (mut arrived, mut reached) = (arrived.unwrap(), reached.unwrap());
    viewer.forget_bound().await;
    assert!(viewer.dialed.lock().unwrap().is_empty());
    let ended = tokio::time::timeout(Duration::from_secs(5), reached.reader.recv()).await;
    assert_eq!(ended.unwrap(), None);
    let ended = tokio::time::timeout(Duration::from_secs(5), arrived.reader.recv()).await;
    assert_eq!(ended.unwrap(), None);

    let old = viewer.bind().await.unwrap();
    let mut changes = viewer.changes();
    viewer.network_changed();
    changes.changed().await.unwrap();
    assert!(!Arc::ptr_eq(&old, &viewer.bind().await.unwrap()));
    drop(old);
    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), second).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address, second),
    );
    let (mut arrived, mut reached) = (arrived.unwrap(), reached.unwrap());
    reached.write(b"typed").await.unwrap();
    assert_eq!(arrived.reader.recv().await.unwrap(), b"typed");
}

fn stun_answer_for(id: [u8; 12], seen: SocketAddr) -> Vec<u8> {
    let SocketAddr::V4(seen) = seen else {
        panic!("the test server listens on IPv4");
    };
    let mut answer = vec![0x01, 0x01, 0x00, 0x0C];
    answer.extend_from_slice(&STUN_COOKIE);
    answer.extend_from_slice(&id);
    answer.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0x00, 0x01]);
    answer.extend_from_slice(&(seen.port() ^ 0x2112).to_be_bytes());
    for (octet, key) in seen.ip().octets().into_iter().zip(STUN_COOKIE) {
        answer.push(octet ^ key);
    }
    answer
}

/// Gives the two devices the allocation of `pipe` on `relay`, as the server's `relayed` does.
#[cfg(feature = "host")]
pub(crate) fn granted(host: &Peers, viewer: &Peers, relay: SocketAddr, pipe: Uuid) {
    for (peers, slot) in [(host, 1), (viewer, 0)] {
        peers.relay_at(Some(relay.to_string()));
        peers.wanted(pipe);
        let (pipe, grant) = Grant::read(&serde_json::json!({
            "type": "relayed",
            "pipe": pipe,
            "allocation": "AQIDBAUGBw==",
            "slot": slot,
            "secret": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "now": 0,
        }))
        .unwrap();
        peers.relayed(pipe, grant);
    }
}

/// Forwards between the two addresses that bound the slots of an allocation, as the Kodosi
/// server's relay does, without checking the binds' signatures.
#[cfg(feature = "host")]
pub(crate) async fn relay_for_tests() -> SocketAddr {
    let socket = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut slots = [None, None];
        let mut packet = [0; 2048];
        loop {
            let (size, from) = socket.recv_from(&mut packet).await.unwrap();
            match packet[0] {
                0x11 => {
                    slots[usize::from(packet[8])] = Some(from);
                    packet[0] = 0x12;
                    socket.send_to(&packet[..9], from).await.unwrap();
                }
                0x10 => {
                    let to = match slots {
                        [Some(first), second] if first == from => second,
                        [first, Some(second)] if second == from => first,
                        _ => None,
                    };
                    if let Some(to) = to {
                        socket.send_to(&packet[..size], to).await.unwrap();
                    }
                }
                _ => {}
            }
        }
    });
    address
}

#[cfg(feature = "host")]
#[tokio::test]
async fn devices_that_find_no_straight_path_meet_through_the_relay() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let relay = relay_for_tests().await;
    let pipe = Uuid::now_v7();
    granted(&host, &viewer, relay, pipe);
    let expected = host.expect(viewer_keys.signing_public(), pipe);
    let (arrived, reached) = tokio::join!(
        expected.meet(&[]),
        viewer.reach(host_keys.signing_public(), &[], pipe),
    );
    let (mut arrived, mut reached) = (arrived.unwrap(), reached.unwrap());
    reached.write(b"through the relay").await.unwrap();
    assert_eq!(arrived.reader.recv().await.unwrap(), b"through the relay");
    arrived.write(b"back").await.unwrap();
    assert_eq!(reached.reader.recv().await.unwrap(), b"back");

    // The route of the last pipe ended; a view opened again at once meets through the relay
    // again before the host stops waiting for it.
    drop((arrived, reached));
    for peers in [&host, &viewer] {
        peers.forget_relayed(pipe);
    }
    let again = Uuid::now_v7();
    viewer.relayed(
        again,
        Grant::read(
            &serde_json::json!({"pipe": again, "allocation": "AQIDBAUGBw==",
        "slot": 0, "secret": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=", "now": 0}),
        )
        .unwrap()
        .1,
    );
    assert!(viewer.grants.lock().unwrap().get(&again).is_none());
    granted(&host, &viewer, relay, again);
    let expected = host.expect(viewer_keys.signing_public(), again);
    let met = tokio::time::timeout(
        ARRIVAL,
        futures_util::future::join(
            expected.meet(&[]),
            viewer.reach(host_keys.signing_public(), &[], again),
        ),
    )
    .await
    .unwrap();
    assert!(met.0.is_some() && met.1.is_some());
}

/// The address at which this computer reaches `peers`.
#[cfg(feature = "host")]
pub(crate) async fn listening(peers: &Arc<Peers>) -> SocketAddr {
    on_this_computer(peers).await[0].0
}

#[tokio::test]
async fn the_quic_socket_learns_how_a_stun_server_sees_it() {
    let server = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    tokio::spawn(async move {
        let mut question = [0; 64];
        let (size, from) = server.recv_from(&mut question).await.unwrap();
        assert_eq!(&question[..2], &[0x00, 0x01]);
        let id = question[8..size].try_into().unwrap();
        let from = SocketAddr::new(from.ip().to_canonical(), from.port());
        server
            .send_to(&stun_answer_for(id, from), from)
            .await
            .unwrap();
    });
    let peers = peers_of(&DeviceKeys::generate().unwrap());
    let bound = peers.bind().await.unwrap();
    let seen = bound.socket.stun(address).await.unwrap();
    assert_eq!(
        seen,
        SocketAddr::from((
            Ipv4Addr::LOCALHOST,
            bound.socket.local_addr().unwrap().port()
        ))
    );
}

#[test]
fn a_stun_answer_with_an_ipv6_address_reads_with_its_question() {
    let id = [9; 12];
    let seen: Ipv6Addr = "2001:db8::7".parse().unwrap();
    let mut answer = vec![0x01, 0x01, 0x00, 0x18];
    answer.extend_from_slice(&STUN_COOKIE);
    answer.extend_from_slice(&id);
    answer.extend_from_slice(&[0x00, 0x20, 0x00, 0x14, 0x00, 0x02]);
    answer.extend_from_slice(&(0xB900_u16 ^ 0x2112).to_be_bytes());
    for (octet, key) in seen.octets().iter().zip(STUN_COOKIE.iter().chain(&id)) {
        answer.push(octet ^ key);
    }
    assert_eq!(
        stun_answer(&answer),
        Some((id, SocketAddr::from((seen, 0xB900))))
    );
    answer[0] = 0x00;
    assert_eq!(stun_answer(&answer), None);
}

#[test]
fn a_private_address_counts_only_on_a_network_of_this_computer() {
    let networks = [
        Network {
            ip: "192.168.1.25".parse().unwrap(),
            mask: "255.255.255.0".parse().unwrap(),
        },
        Network {
            ip: "2001:db8:1::5".parse().unwrap(),
            mask: "ffff:ffff:ffff:ffff::".parse().unwrap(),
        },
    ];
    let theirs = [
        "203.0.113.9:1",
        "192.168.1.30:1",
        "10.0.0.5:1",
        "169.254.1.1:1",
        "127.0.0.1:1",
        "192.168.1.31:0",
        "[fe80::1]:1",
        "[fd00::1]:1",
        "[2001:db8:2::9]:1",
        "[::ffff:192.168.1.32]:1",
    ]
    .map(|address| address.parse::<SocketAddr>().unwrap());
    let address = |text: &str| text.parse::<SocketAddr>().unwrap();
    assert_eq!(
        usable(&theirs, &networks, true),
        [
            (address("192.168.1.30:1"), true),
            (address("192.168.1.32:1"), true),
            (address("[2001:db8:2::9]:1"), false),
            (address("203.0.113.9:1"), false),
        ]
    );
    assert_eq!(
        usable(&theirs, &networks, false),
        [
            (address("192.168.1.30:1"), true),
            (address("192.168.1.32:1"), true),
            (address("203.0.113.9:1"), false),
        ]
    );
}

#[cfg(feature = "host")]
#[tokio::test]
async fn video_frames_go_as_datagrams_once_the_viewer_takes_the_flow() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let address = on_this_computer(&host).await;
    let pipe = Uuid::now_v7();
    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), pipe).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address, pipe),
    );
    let (arrived, reached) = (arrived.unwrap(), reached.unwrap());
    let (document, mut heard) = mpsc::channel(16);
    let flow = super::super::media::Flow::start(&arrived.media().unwrap(), document).unwrap();
    let (frames, mut shown) = mpsc::channel(16);
    let _view =
        super::super::media::receive(&reached.media().unwrap(), flow.number, frames).unwrap();

    let mut frame = vec![stream::viewer::FRAME, 0, 0, 0, 1, 1, 0];
    frame.extend((0..20_000_u32).map(|byte| byte.to_be_bytes()[3]));
    let sent = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if flow.take(frame.clone()).is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(sent.is_ok(), "the viewer's greeting readies the flow");
    let arrived = tokio::time::timeout(Duration::from_secs(5), shown.recv()).await;
    assert_eq!(arrived.unwrap().unwrap(), frame);
    let rate = tokio::time::timeout(Duration::from_secs(5), heard.recv()).await;
    let rate = rate.unwrap().unwrap();
    assert_eq!(rate.first(), Some(&stream::viewer::JSON));
    assert!(String::from_utf8_lossy(&rate[1..]).contains("\"rate\""));

    // Both sides rest while the picture is still, and wake for the next frame.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut next = vec![stream::viewer::FRAME, 0, 0, 0, 2, 0, 0];
    next.extend((0..5_000_u32).map(|byte| byte.to_be_bytes()[3]));
    assert!(flow.take(next.clone()).is_ok());
    let arrived = tokio::time::timeout(Duration::from_secs(5), shown.recv()).await;
    assert_eq!(arrived.unwrap().unwrap(), next);
}

/// The stream repairs at once when its frame cannot even wait for the connection, and hears of
/// each such frame however full its own queue is.
#[cfg(feature = "host")]
#[tokio::test]
async fn a_frame_that_the_flow_has_no_room_for_is_told_lost_to_the_stream() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let address = on_this_computer(&host).await;
    let pipe = Uuid::now_v7();
    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), pipe).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address, pipe),
    );
    let (arrived, reached) = (arrived.unwrap(), reached.unwrap());
    let (document, mut heard) = mpsc::channel(1);
    let flow = super::super::media::Flow::start(&arrived.media().unwrap(), document).unwrap();
    let (frames, _shown) = mpsc::channel(64);
    let _view =
        super::super::media::receive(&reached.media().unwrap(), flow.number, frames).unwrap();
    let frame = |id: u32| {
        let mut frame = vec![stream::viewer::FRAME];
        frame.extend(id.to_be_bytes());
        frame.extend([0, 0]);
        frame.extend([0; 2000]);
        frame
    };
    let ready = tokio::time::timeout(Duration::from_secs(5), async {
        while flow.take(frame(1)).is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(ready.is_ok(), "the viewer's greeting readies the flow");

    // The send task does not run between these, so its inbox fills behind frame 1.
    for id in 2..=10 {
        assert!(flow.take(frame(id)).is_ok());
    }
    let mut lost = Vec::new();
    let heard_until = tokio::time::Instant::now() + Duration::from_secs(1);
    while let Ok(Some(message)) = tokio::time::timeout_at(heard_until, heard.recv()).await {
        if let Some(said) = stream::viewer::from_json::<Value>(&message)
            && said["type"] == "lost"
        {
            lost.push(said["frame"].as_u64().unwrap());
        }
    }
    assert_eq!(lost, [5, 6, 7, 8, 9, 10]);
}

/// A frame that the viewer put together waits for room in the pipe to its app: the host took it as
/// arrived, so it must reach the app and be acknowledged.
#[cfg(feature = "host")]
#[tokio::test]
async fn a_frame_that_arrived_waits_for_the_app_to_take_it() {
    let (host_keys, viewer_keys) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (host, viewer) = (peers_of(&host_keys), peers_of(&viewer_keys));
    let address = on_this_computer(&host).await;
    let pipe = Uuid::now_v7();
    let (arrived, reached) = tokio::join!(
        host.expect(viewer_keys.signing_public(), pipe).meet(&[]),
        viewer.reach_at(host_keys.signing_public(), address, pipe),
    );
    let (arrived, reached) = (arrived.unwrap(), reached.unwrap());
    let (document, _heard) = mpsc::channel(64);
    let flow = super::super::media::Flow::start(&arrived.media().unwrap(), document).unwrap();
    let (frames, mut shown) = mpsc::channel(1);
    let _view =
        super::super::media::receive(&reached.media().unwrap(), flow.number, frames).unwrap();
    let frame = |id: u32| {
        let mut frame = vec![stream::viewer::FRAME];
        frame.extend(id.to_be_bytes());
        frame.extend([u8::from(id == 1), 0]);
        frame.extend([0; 2000]);
        frame
    };
    let ready = tokio::time::timeout(Duration::from_secs(5), async {
        while flow.take(frame(1)).is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(ready.is_ok(), "the viewer's greeting readies the flow");
    for id in 2..=4 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(flow.take(frame(id)).is_ok());
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    for id in 1..=4 {
        let taken = tokio::time::timeout(Duration::from_secs(5), shown.recv()).await;
        assert_eq!(taken.unwrap().unwrap(), frame(id));
    }
}
