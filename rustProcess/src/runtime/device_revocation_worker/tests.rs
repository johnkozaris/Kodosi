use super::*;
use crate::{config::AppConfig, identity_core::device_keys::generate_device_keys_for_test};
use kodosi_domain::auth::AuthState;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const USER: &str = "11111111-1111-4111-8111-111111111111";

fn runtime(api: &str) -> Runtime {
    let mut config = AppConfig::default();
    config.backend.api = Some(api.to_owned());
    config.auth.keyring_service = format!("kodosi.revoke.test.{}", uuid::Uuid::now_v7());
    let mut app = Runtime::with_dependencies(
        config.clone(),
        CancellationToken::new(),
        super::super::RuntimeDependencies::isolated(&config).unwrap(),
    )
    .unwrap();
    app.state.identity.advance_account_epoch().unwrap();
    app.state.identity.auth = AuthState::Authenticated {
        subject: Some(USER.try_into().unwrap()),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    app.set_remote_surfaces_ready_for_test();
    app
}

#[tokio::test]
async fn stalled_network_does_not_occupy_reducer_and_epoch_change_cancels_immediately() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        accepted_tx.send(()).unwrap();
        std::future::pending::<()>().await;
        drop(socket);
    });
    let mut app = runtime(&format!("http://{address}/"));
    let keys = generate_device_keys_for_test(USER).unwrap();
    app.device_key_store.save_for_test(USER, &keys).unwrap();
    let mut worker = DeviceRevocationWorker::default();
    worker
        .start(&app, "another-device".to_owned(), Reply::Desktop)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), accepted_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(
        worker
            .start(&app, "third-device".to_owned(), Reply::Desktop)
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), worker.poll())
            .await
            .is_err()
    );
    let task = worker.pending.as_ref().unwrap().task.abort_handle();
    super::super::auth::set_signed_out(&mut app).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    worker.reconcile_account(&app);
    assert!(worker.pending.is_none());
    assert!(app.state.runtime_outbox.drain_devices().is_empty());
    server.abort();
}

#[tokio::test]
async fn current_unauthorized_completion_expires_auth_but_stale_completion_does_not() {
    for stale in [false, true] {
        let mut app = runtime("http://127.0.0.1:9/");
        let (_, epoch) = app.state.identity.event_context();
        let mut worker = DeviceRevocationWorker {
            pending: Some(Pending {
                user_id: USER.to_owned(),
                epoch,
                reply: Reply::Desktop,
                task: tokio::spawn(std::future::pending()),
            }),
        };
        if stale {
            app.state.identity.advance_account_epoch().unwrap();
        }
        worker.finish(
            &mut app,
            Ok(Completion {
                outcome: Err(AppError::Unauthorized),
                events: vec![],
            }),
        );
        assert_eq!(app.state.identity.auth.is_authenticated(), stale);
        assert_eq!(
            app.state.runtime_outbox.drain_devices().len(),
            usize::from(!stale)
        );
    }
}

#[tokio::test]
async fn successful_completion_invalidates_hosted_keys_before_publishing_inventory() {
    use crate::sharing::shared_session_registry::SharedSessionState;
    use kodosi_domain::{ids::SessionId, permissions::ShareScope};
    let mut app = runtime("http://127.0.0.1:9/");
    let id = SessionId::new();
    app.state.sharing.shared_sessions.insert(
        id,
        SharedSessionState::new(
            id.to_string(),
            uuid::Uuid::now_v7(),
            "test".to_owned(),
            ShareScope::MyDevices,
            None,
            Some([7; 32]),
            Some(1),
        ),
    );
    let (_, epoch) = app.state.identity.event_context();
    let mut worker = DeviceRevocationWorker {
        pending: Some(Pending {
            user_id: USER.to_owned(),
            epoch,
            reply: Reply::Desktop,
            task: tokio::spawn(std::future::pending()),
        }),
    };
    worker.finish(
        &mut app,
        Ok(Completion {
            outcome: Ok(DeviceRevocationOutcome {
                history_warning: None,
                #[cfg(feature = "cli")]
                revoked_device_id: "old".to_owned(),
                #[cfg(feature = "cli")]
                new_generation: 2,
            }),
            events: vec![DeviceEvent::List {
                self_device_id: "current".to_owned(),
                local_device_enrolled: true,
                devices: vec![],
            }],
        }),
    );
    assert!(
        app.state
            .sharing
            .shared_sessions
            .get(id)
            .unwrap()
            .session_key()
            .is_none()
    );
    assert!(matches!(
        app.state.runtime_outbox.drain_devices().as_slice(),
        [DeviceEvent::List { .. }]
    ));
}
