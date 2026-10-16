use super::*;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

fn namespace() -> String {
    format!("test-{}-{}", std::process::id(), rand_suffix())
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        % 1_000_000_007
}

#[test]
fn endpoint_names_are_validated() {
    assert!(Endpoint::new("", "host").is_err());
    assert!(Endpoint::new("a/b", "host").is_err());
    assert!(Endpoint::new("abc", "host.sock").is_err());
    assert!(Endpoint::new(&"a".repeat(65), "host").is_err());
    assert!(Endpoint::new("abc-123", "host").is_ok());
}

#[tokio::test]
async fn a_listener_serves_its_user_and_refuses_a_second_owner() {
    let endpoint = Endpoint::new(&namespace(), "host").unwrap();
    assert_eq!(
        connect(&endpoint).await.err().map(|error| error.kind()),
        Some(io::ErrorKind::NotFound)
    );
    let mut listener = Listener::bind(&endpoint).await.unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let stream = listener.accept().await.unwrap();
            assert_eq!(stream.peer_pid(), Some(std::process::id()));
            let (mut reader, mut writer) = stream.into_split();
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte).await.unwrap();
            writer.write_all(&[byte[0] + 1]).await.unwrap();
        }
        listener
    });
    for value in [1u8, 7] {
        let mut client = connect(&endpoint).await.unwrap();
        assert_eq!(client.peer_pid(), Some(std::process::id()));
        client.write_all(&[value]).await.unwrap();
        let mut reply = [0u8; 1];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], value + 1);
    }
    let listener = server.await.unwrap();
    assert_eq!(
        Listener::bind(&endpoint)
            .await
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::AddrInUse)
    );
    drop(listener);
    assert!(connect(&endpoint).await.is_err());
    drop(Listener::bind(&endpoint).await.unwrap());
}
