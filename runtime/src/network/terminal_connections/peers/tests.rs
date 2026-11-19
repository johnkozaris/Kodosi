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

/// An Apple socket gives a refused datagram as a failed read, as it gives a socket that iOS
/// closed in the background.
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

    let socket = Arc::clone(&viewer.bind().await.unwrap().socket);
    let free = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let refusing = if socket.dual {
        SocketAddr::new(IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()), free)
    } else {
        SocketAddr::from((Ipv4Addr::LOCALHOST, free))
    };
    socket.inner.io.connect(refusing).await.unwrap();
    socket.inner.io.send(&[0]).await.unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), reached.reader.recv()).await;
    assert_eq!(ended.unwrap(), None);

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
