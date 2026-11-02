use super::{super::channel::tests::channel_keys, *};
use crate::identity::keys::DeviceKeys;

#[cfg(feature = "host")]
struct Sides {
    host: DeviceKeys,
    viewer: DeviceKeys,
    requests: mpsc::Receiver<HostRequest>,
    door: Door,
    authorization: CancellationToken,
}

#[cfg(feature = "host")]
fn sides() -> Sides {
    let (host, viewer) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let (requests_to, requests) = mpsc::channel(8);
    let authorization = CancellationToken::new();
    let door = Door::new(
        Arc::new(channel_keys(&host)),
        viewer.signing_public().to_vec(),
        requests_to,
        Uuid::now_v7(),
        authorization.clone(),
    );
    Sides {
        host,
        viewer,
        requests,
        door,
        authorization,
    }
}

#[cfg(feature = "host")]
fn host_as_seen_by(sides: &Sides, viewer: &DeviceKeys) -> Host {
    Host::new(
        Arc::new(channel_keys(viewer)),
        "host-device".to_owned(),
        sides.host.signing_public().to_vec(),
    )
}

#[cfg(feature = "host")]
/// Answers the request for a document with two ends that the test holds.
async fn document(
    requests: &mut mpsc::Receiver<HostRequest>,
) -> (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) {
    let Some(HostRequest::Document {
        reply, key, name, ..
    }) = requests.recv().await
    else {
        panic!("no document was asked for");
    };
    assert_eq!((key.as_str(), name.as_str()), (BROWSER_PIPE, "Ana"));
    let (to_viewer, receiver) = mpsc::channel(8);
    let (sender, from_viewer) = mpsc::channel(8);
    reply.send(Ok(PipeEnds { sender, receiver })).ok().unwrap();
    (to_viewer, from_viewer)
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_pipe_goes_straight_to_the_host_and_ends_with_either_side() {
    let mut sides = sides();
    let addresses = sides.door.open().await;
    let host = host_as_seen_by(&sides, &sides.viewer);
    let mut channel = host.reach(&addresses).await.unwrap();
    let id = Uuid::now_v7();
    channel
        .send(&Frame::PipeOpen {
            pipe: id,
            key: BROWSER_PIPE.to_owned(),
            name: "Ana".to_owned(),
        })
        .await
        .unwrap();
    let (to_viewer, mut from_viewer) = document(&mut sides.requests).await;

    let (to_view, mut at_view) = mpsc::channel(8);
    let (from_view, leaving) = mpsc::channel(8);
    let carried = tokio::spawn(async move {
        let ends = PipeEnds {
            sender: to_view,
            receiver: leaving,
        };
        carry(&mut channel, id, ends).await
    });

    let large = vec![7; wire::PIPE_PART * 3 + 5];
    to_viewer.send(large.clone()).await.unwrap();
    assert_eq!(at_view.recv().await.unwrap(), large);
    from_view.send(b"a key".to_vec()).await.unwrap();
    assert_eq!(from_viewer.recv().await.unwrap(), b"a key");

    drop(to_viewer);
    carried.await.unwrap().unwrap();
    assert!(at_view.recv().await.is_none());
}

#[cfg(feature = "host")]
#[tokio::test]
async fn only_the_device_of_the_viewer_gets_through() {
    let sides = sides();
    let addresses = sides.door.open().await;
    let stranger = DeviceKeys::generate().unwrap();
    let host = host_as_seen_by(&sides, &stranger);
    let mut tried = host.reach(&addresses).await;
    if let Some(channel) = &mut tried {
        let open = Frame::PipeOpen {
            pipe: Uuid::now_v7(),
            key: BROWSER_PIPE.to_owned(),
            name: "Mallory".to_owned(),
        };
        drop(channel.send(&open).await);
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), channel.receive()).await,
            Ok(Err(_) | Ok(None))
        ));
    }
    let port = host_as_seen_by(&sides, &sides.viewer)
        .reach(&addresses)
        .await;
    assert!(port.is_some());
}

#[cfg(feature = "host")]
#[tokio::test]
async fn a_viewer_that_lost_its_access_loses_the_straight_connection() {
    let mut sides = sides();
    let addresses = sides.door.open().await;
    let host = host_as_seen_by(&sides, &sides.viewer);
    let mut channel = host.reach(&addresses).await.unwrap();
    channel
        .send(&Frame::PipeOpen {
            pipe: Uuid::now_v7(),
            key: BROWSER_PIPE.to_owned(),
            name: "Ana".to_owned(),
        })
        .await
        .unwrap();
    let _ends = document(&mut sides.requests).await;
    sides.authorization.cancel();
    let ended = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match channel.receive().await {
                Ok(Some(_)) => {}
                ended => return ended.ok().flatten().is_none(),
            }
        }
    })
    .await;
    assert_eq!(ended, Ok(true));
}

#[tokio::test]
async fn a_browser_uses_the_terminal_channel_when_the_host_cannot_be_reached() {
    let (commands, mut requests) = mpsc::channel(8);
    let (to_view, _at_view) = mpsc::channel(8);
    let (from_view, leaving) = mpsc::channel(8);
    let pipe = Uuid::now_v7();
    tokio::spawn(run(Pipe {
        commands,
        cancel: CancellationToken::new(),
        id: pipe,
        key: BROWSER_PIPE.to_owned(),
        name: "Ana".to_owned(),
        to_view,
        from_view: leaving,
    }));
    let Some(RemoteRequest::DirectAsk { reply, .. }) = requests.recv().await else {
        panic!("the browser did not ask for a straight connection");
    };
    let (host, viewer) = (
        DeviceKeys::generate().unwrap(),
        DeviceKeys::generate().unwrap(),
    );
    let host = Host::new(
        Arc::new(channel_keys(&viewer)),
        "host-device".to_owned(),
        host.signing_public().to_vec(),
    );
    reply.send((Arc::new(host), Vec::new())).ok().unwrap();
    assert!(matches!(
        requests.recv().await,
        Some(RemoteRequest::PipeOpen { key, .. }) if key == BROWSER_PIPE
    ));
    from_view.send(b"a key".to_vec()).await.unwrap();
    assert!(matches!(
        requests.recv().await,
        Some(RemoteRequest::PipeData { more: false, .. })
    ));
}

#[tokio::test]
async fn a_document_stays_on_the_terminal_channel() {
    let (commands, mut requests) = mpsc::channel(8);
    let (to_view, _at_view) = mpsc::channel(8);
    let (_from_view, leaving) = mpsc::channel(8);
    tokio::spawn(run(Pipe {
        commands,
        cancel: CancellationToken::new(),
        id: Uuid::now_v7(),
        key: "/work/plan.docx".to_owned(),
        name: "Ana".to_owned(),
        to_view,
        from_view: leaving,
    }));
    assert!(matches!(
        requests.recv().await,
        Some(RemoteRequest::PipeOpen { .. })
    ));
}
