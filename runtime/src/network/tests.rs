use super::*;

#[cfg(feature = "host")]
#[test]
fn the_server_hears_the_agent_and_its_state_but_not_its_text() {
    use crate::terminal::{
        BlockedKind, ContextPlace, PlaceKind, ProgramState, ProgramStatus, TerminalContext,
        TerminalMetadata,
    };
    let status = TerminalStatus::of(&TerminalMetadata {
        connected_users: vec![],
        directory: Some("/Users/someone/private".into()),
        title: Some("A private title".into()),
        program: Some("claude".into()),
        account: Some(2),
        settings: None,
        pending: None,
        status: Some(ProgramStatus {
            state: ProgramState::Blocked,
            kind: Some(BlockedKind::Permission),
            progress: None,
            title: Some("Bash".into()),
            message: Some("rm -rf build".into()),
        }),
        context: Some(TerminalContext {
            place: Some(ContextPlace {
                kind: PlaceKind::Remote,
                name: "private-server".into(),
            }),
            root: true,
        }),
        canvas: None,
        face: None,
        helpers: 0,
        look: crate::terminal::TerminalLook::Light,
        stopped: true,
        changes: Some(crate::worktree::WorktreeChanges {
            added: 120,
            removed: 34,
        }),
    });
    let bytes = serde_json::to_vec(&status).unwrap();
    assert_eq!(
        bytes,
        br#"{"program":"claude","account":2,"state":"blocked","kind":"permission"}"#
    );
    let notice = |status: Value| {
        wire::TerminalNotice::read(&serde_json::json!({"sessionId": Uuid::nil(),
            "incarnationId": Uuid::nil(), "online": true, "status": status}))
    };
    assert_eq!(
        notice(serde_json::from_slice(&bytes).unwrap())
            .unwrap()
            .status,
        Some(status)
    );
    for ignored in [
        serde_json::json!({"program":"other"}),
        serde_json::json!({"state":"working","kind":"permission"}),
        serde_json::json!({"account":1,"state":"working"}),
    ] {
        assert_eq!(notice(ignored).unwrap().status, None);
    }
    assert!(notice(serde_json::json!({"state":"blocked","message":"Allow?"})).is_err());
}

#[tokio::test]
async fn a_connection_that_drops_during_a_request_has_a_clear_text() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        drop(tokio::io::AsyncReadExt::read(&mut socket, &mut request).await);
    });
    let error = Error::from(
        reqwest::Client::new()
            .get(format!("http://{address}/v1/sessions"))
            .send()
            .await
            .unwrap_err(),
    );
    server.await.unwrap();
    assert!(matches!(error, Error::Dropped), "{error}");
    assert!(error.unanswered());
    assert!(!error.to_string().contains("http"));
}
