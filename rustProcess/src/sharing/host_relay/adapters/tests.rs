use tokio::sync::mpsc;

use super::*;
use crate::{
    local_sessions::size_authority::SizeOrigin,
    session_runtime::{
        events::AccountEpoch,
        handles::{SessionRuntimeHandle, SessionScreenInstruction, SessionSenders},
    },
};
use kodosi_domain::ids::SessionId;

fn decision_reply() -> kodosi_backend_client::relay::SemanticAdmissionReply {
    kodosi_backend_client::relay::SemanticAdmissionReply::channel().0
}

fn account_origin() -> AccountEventOrigin {
    AccountEventOrigin {
        account_user_id: "11111111-1111-1111-1111-111111111111".to_owned(),
        epoch: AccountEpoch::INITIAL.next().expect("test account epoch"),
    }
}

#[test]
fn every_host_event_keeps_exact_origin_and_rejects_cross_session_targets() {
    let session_id = SessionId::new();
    let wrong_session_id = SessionId::new();
    let host_origin = HostRelayEventOrigin {
        account_origin: account_origin(),
        session_id,
        relay_generation: 7,
    };
    let events = vec![
        HostRelayEvent::Info {
            id: session_id,
            message: "info".to_owned(),
        },
        HostRelayEvent::LogError {
            id: session_id,
            message: "error".to_owned(),
        },
        HostRelayEvent::BackendAccessInvalid {
            reason: "expired".to_owned(),
        },
        HostRelayEvent::HostDemand {
            id: session_id,
            required: true,
            participant_count: 1,
            reason: "viewer".to_owned(),
        },
        HostRelayEvent::StateChanged {
            id: session_id,
            state: kodosi_domain::session::SessionState::Running,
        },
        HostRelayEvent::HostAccessRevoked {
            id: session_id,
            revoked_user_id: "revoked-user".to_owned(),
        },
        HostRelayEvent::BackendRelayRestarted { id: session_id },
        HostRelayEvent::HostKeyDistributionRequested {
            id: session_id,
            fence_id: "fence".to_owned(),
        },
        HostRelayEvent::HostKeyRotationRequired {
            id: session_id,
            reason: "watermark".to_owned(),
            fail_closed: false,
        },
        HostRelayEvent::ActionCompleted {
            id: session_id,
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: "action-result".to_owned(),
            request_id: "request-result".to_owned(),
            requester_user_id: "user".to_owned(),
            requester_device_id: "device".to_owned(),
            accepted: true,
        },
        HostRelayEvent::PermissionDecision {
            id: session_id,
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: "action".to_owned(),
            request_id: "tool".to_owned(),
            request_generation: 7,
            decision: "allow".to_owned(),
            decider_user_id: "user".to_owned(),
            decider_device_id: Some("device".to_owned()),
            reply: decision_reply(),
        },
    ];
    for event in events {
        let mapped = map_host_relay_event(event, &host_origin).expect("matching event maps");
        assert_eq!(
            mapped.account_event_origin(),
            Some(&host_origin.account_origin)
        );
        assert_eq!(mapped.host_relay_event_origin(), Some(&host_origin));
    }

    let mismatched_events = vec![
        HostRelayEvent::Info {
            id: wrong_session_id,
            message: String::new(),
        },
        HostRelayEvent::LogError {
            id: wrong_session_id,
            message: String::new(),
        },
        HostRelayEvent::HostDemand {
            id: wrong_session_id,
            required: false,
            participant_count: 0,
            reason: String::new(),
        },
        HostRelayEvent::StateChanged {
            id: wrong_session_id,
            state: kodosi_domain::session::SessionState::Running,
        },
        HostRelayEvent::HostAccessRevoked {
            id: wrong_session_id,
            revoked_user_id: "revoked-user".to_owned(),
        },
        HostRelayEvent::BackendRelayRestarted {
            id: wrong_session_id,
        },
        HostRelayEvent::HostKeyDistributionRequested {
            id: wrong_session_id,
            fence_id: String::new(),
        },
        HostRelayEvent::HostKeyRotationRequired {
            id: wrong_session_id,
            reason: String::new(),
            fail_closed: true,
        },
        HostRelayEvent::ActionCompleted {
            id: wrong_session_id,
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: String::new(),
            request_id: String::new(),
            requester_user_id: String::new(),
            requester_device_id: String::new(),
            accepted: false,
        },
        HostRelayEvent::PermissionDecision {
            id: wrong_session_id,
            incarnation_id: uuid::Uuid::now_v7(),
            action_id: String::new(),
            request_id: String::new(),
            request_generation: 7,
            decision: "deny".to_owned(),
            decider_user_id: String::new(),
            decider_device_id: None,
            reply: decision_reply(),
        },
    ];
    for event in mismatched_events {
        assert!(map_host_relay_event(event, &host_origin).is_none());
    }
}

#[tokio::test]
async fn owner_input_payload_is_batched_into_one_screen_instruction() {
    let (screen_tx, mut screen_rx) = mpsc::channel(1);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));

    KodosiHostRelayPort::new(runtime, Arc::default())
        .dispatch_input_payload(b"pwd\r\t\x1b\x08", true)
        .await
        .unwrap_or_else(|error| panic!("dispatch should succeed: {error}"));

    let Some(SessionScreenInstruction::InputBatch(batch)) = screen_rx.recv().await else {
        panic!("expected one batched screen instruction");
    };
    assert_eq!(batch, vec![SessionInput::new(b"pwd\r\t\x1b\x08".to_vec())]);
    assert!(
        screen_rx.try_recv().is_err(),
        "should only queue one instruction"
    );
}

#[tokio::test]
async fn owner_input_payload_preserves_all_binary_values_in_one_instruction() {
    let (screen_tx, mut screen_rx) = mpsc::channel(1);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));
    let payload = (0_u8..=u8::MAX).collect::<Vec<_>>();

    KodosiHostRelayPort::new(runtime, Arc::default())
        .dispatch_input_payload(&payload, true)
        .await
        .unwrap_or_else(|error| panic!("dispatch should succeed: {error}"));

    let Some(SessionScreenInstruction::InputBatch(batch)) = screen_rx.recv().await else {
        panic!("expected one batched screen instruction");
    };
    assert_eq!(batch, vec![SessionInput::new(payload)]);
    assert!(screen_rx.try_recv().is_err());
}

#[tokio::test]
async fn remote_input_reclaims_with_remembered_size_in_one_instruction() {
    let (screen_tx, mut screen_rx) = mpsc::channel(1);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));
    let authority = Arc::new(SizeAuthorityCell::default());
    let remote_size = TerminalSize::new(40, 120).expect("remote size");
    assert!(authority.admit_resize(SizeOrigin::Remote, remote_size));
    authority.begin_claim(SizeOrigin::Local).await.commit();

    KodosiHostRelayPort::new(runtime, Arc::clone(&authority))
        .dispatch_input_payload(b"remote", true)
        .await
        .unwrap_or_else(|error| panic!("dispatch should succeed: {error}"));

    let Some(SessionScreenInstruction::ResizeAndInputBatch { size, inputs }) =
        screen_rx.recv().await
    else {
        panic!("expected one atomic reclaim instruction");
    };
    assert_eq!(size, remote_size);
    assert_eq!(inputs, vec![SessionInput::new(b"remote".to_vec())]);
    assert!(screen_rx.try_recv().is_err());
    assert!(!authority.admit_resize(
        SizeOrigin::Local,
        TerminalSize::new(30, 100).expect("local size")
    ));
}

#[tokio::test]
async fn failed_remote_input_admission_does_not_steal_authority() {
    let (screen_tx, screen_rx) = mpsc::channel(1);
    drop(screen_rx);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));
    let authority = Arc::new(SizeAuthorityCell::default());
    assert!(authority.admit_resize(
        SizeOrigin::Remote,
        TerminalSize::new(40, 120).expect("remote size")
    ));
    authority.begin_claim(SizeOrigin::Local).await.commit();

    let result = KodosiHostRelayPort::new(runtime, Arc::clone(&authority))
        .dispatch_input_payload(b"remote", true)
        .await;

    assert!(result.is_err());
    assert!(!authority.admit_resize(
        SizeOrigin::Remote,
        TerminalSize::new(41, 121).expect("new remote size")
    ));
}

#[tokio::test]
async fn owner_resize_waits_for_screen_application() {
    let (screen_tx, mut screen_rx) = mpsc::channel(1);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));
    let size = TerminalSize::new(24, 80).expect("valid size");
    let port = KodosiHostRelayPort::new(runtime, Arc::default());

    let resize = port.resize(size, None, true, false);
    let apply = async {
        let Some(SessionScreenInstruction::Resize {
            size: observed,
            pixel_geometry: None,
            completion: Some(completion),
        }) = screen_rx.recv().await
        else {
            panic!("expected a correlated resize screen instruction");
        };
        assert_eq!(observed, size);
        completion.send(Ok(())).expect("resize waiter remains live");
    };
    let (result, ()) = tokio::join!(resize, apply);
    result.unwrap_or_else(|error| panic!("resize should succeed: {error}"));
    assert!(screen_rx.try_recv().is_err());
}

#[tokio::test]
async fn owner_focus_queues_screen_focus_instruction() {
    let (screen_tx, mut screen_rx) = mpsc::channel(1);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));

    KodosiHostRelayPort::new(runtime, Arc::default())
        .set_focus("client-7".to_owned(), ClientFocus::Focused)
        .await
        .unwrap_or_else(|error| panic!("focus should succeed: {error}"));

    let Some(SessionScreenInstruction::Focus { client_id }) = screen_rx.recv().await else {
        panic!("expected a focus screen instruction");
    };
    assert_eq!(client_id, "client-7");
    assert!(
        screen_rx.try_recv().is_err(),
        "should only queue one instruction"
    );
}

#[tokio::test]
async fn owner_blur_queues_screen_blur_instruction() {
    let (screen_tx, mut screen_rx) = mpsc::channel(1);
    let runtime =
        SessionRuntimeHandle::new(SessionId::new(), SessionSenders::new(Some(screen_tx), None));

    KodosiHostRelayPort::new(runtime, Arc::default())
        .set_focus("client-9".to_owned(), ClientFocus::Blurred)
        .await
        .unwrap_or_else(|error| panic!("blur should succeed: {error}"));

    let Some(SessionScreenInstruction::Blur { client_id }) = screen_rx.recv().await else {
        panic!("expected a blur screen instruction");
    };
    assert_eq!(client_id, "client-9");
    assert!(
        screen_rx.try_recv().is_err(),
        "should only queue one instruction"
    );
}
