use super::{
    AccessGrantEntry, AccountSessionEvent, AuthEvent, LocalSessionListEntry, PermissionFlags,
    RelayActionStatus, RemoteSessionListEntry, RoomListEntry, RuntimeSessionStatus, SessionCommand,
    SessionEvent, SessionListEntry, SessionListEntryMeta, SessionSemanticActions, TerminalCommand,
    TerminalEvent,
    authority::{render_desktop_runtime_authority_json, sample_runtime_local_session_info},
};
use kodosi_domain::{
    lifecycle::{ConnectionState, RemoteSessionAccessIssue, RemoteSessionAccessState},
    permissions::{AccessLevel, ShareScope},
    session::SessionMode,
};

#[test]
fn accepts_modern_session_create_messages() {
    let payload = r#"{"type":"session.create","requestId":"req-123","name":"session-123","workingDir":"/tmp/project"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("modern session.create should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::Create {
            request_id,
            name,
            working_dir,
            resume: _,
        } => {
            assert_eq!(request_id, "req-123");
            assert_eq!(name, "session-123");
            assert_eq!(working_dir.as_deref(), Some("/tmp/project"));
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn retired_session_input_is_rejected() {
    let payload = r#"{"type":"session.input","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","keys":"ls"}"#;
    assert!(serde_json::from_str::<TerminalCommand>(payload).is_err());
}

#[test]
fn session_input_bytes_requires_incarnation() {
    let missing = r#"{"type":"session.inputBytes","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","bytes":[27,91,65]}"#;
    assert!(serde_json::from_str::<TerminalCommand>(missing).is_err());

    let payload = r#"{"type":"session.inputBytes","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","bytes":[27,91,65],"expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000001"}"#;
    let envelope: TerminalCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("fenced session.inputBytes should deserialize: {error}"),
    };

    match envelope {
        TerminalCommand::InputBytes {
            session_id,
            bytes,
            expected_runtime_incarnation_id,
            ..
        } => {
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
            assert_eq!(bytes, b"\x1b[A");
            assert_eq!(
                expected_runtime_incarnation_id,
                "01900000-0000-7000-8000-000000000001"
            );
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn accepts_session_rename_with_session_id() {
    let payload = r#"{"type":"session.rename","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","name":"renamed-session"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("session.rename should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::Rename { session_id, name } => {
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
            assert_eq!(name, "renamed-session");
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn accepts_session_mode_with_session_id() {
    let payload = r#"{"type":"session.mode","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000001","mode":"plan"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("session.mode should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::SetMode {
            session_id,
            expected_runtime_incarnation_id,
            mode,
        } => {
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
            assert_eq!(
                expected_runtime_incarnation_id,
                "01900000-0000-7000-8000-000000000001"
            );
            assert_eq!(mode, SessionMode::Plan);
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn retired_session_use_my_size_is_rejected() {
    let payload = r#"{"type":"session.useMySize","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","cols":120,"rows":37}"#;
    assert!(serde_json::from_str::<SessionCommand>(payload).is_err());
}

#[test]
fn session_resize_requires_complete_v17_identity() {
    let incomplete = r#"{"type":"session.resize","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","cols":120,"rows":37}"#;
    assert!(serde_json::from_str::<TerminalCommand>(incomplete).is_err());
}

#[test]
fn accepts_exact_v17_session_resize_identity() {
    let payload = r#"{"type":"session.resize","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","requestId":"01900000-0000-7000-8000-000000000002","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000001","subscriptionId":"terminal-1","subscriptionGeneration":7,"surfaceGeneration":3,"cols":120,"rows":37,"widthPixels":1200,"heightPixels":740,"cellWidthPixels":10,"cellHeightPixels":20,"claim":true}"#;
    let envelope: TerminalCommand = serde_json::from_str(payload)
        .unwrap_or_else(|error| panic!("exact session.resize should deserialize: {error}"));

    let TerminalCommand::Resize {
        session_id,
        identity,
        claim,
    } = envelope
    else {
        panic!("expected session.resize");
    };
    assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
    assert!(claim);
    assert_eq!(identity.request_id, "01900000-0000-7000-8000-000000000002");
    assert_eq!(
        identity.expected_runtime_incarnation_id,
        "01900000-0000-7000-8000-000000000001"
    );
    assert_eq!(identity.subscription_id, "terminal-1");
    assert_eq!(identity.subscription_generation, 7);
    assert_eq!(identity.surface_generation, 3);
    assert_eq!((identity.cols, identity.rows), (120, 37));
    assert_eq!(
        (
            identity.width_pixels,
            identity.height_pixels,
            identity.cell_width_pixels,
            identity.cell_height_pixels,
        ),
        (1_200, 740, 10, 20)
    );
}

#[test]
fn accepts_session_scope_with_runtime_scope() {
    let payload = r#"{"type":"session.scope","requestId":"req-1","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011","scope":"room"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("session.scope should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::SetShareScope {
            request_id: _,
            session_id,
            expected_runtime_incarnation_id: _,
            scope,
            room_id,
        } => {
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
            assert_eq!(scope, ShareScope::Room);
            assert_eq!(room_id, None);
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn accepts_session_scope_with_room_id() {
    let payload = r#"{"type":"session.scope","requestId":"req-2","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011","scope":"room","roomId":"room-123"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("session.scope with roomId should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::SetShareScope {
            request_id: _,
            session_id,
            expected_runtime_incarnation_id: _,
            scope,
            room_id,
        } => {
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
            assert_eq!(scope, ShareScope::Room);
            assert_eq!(room_id.as_deref(), Some("room-123"));
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn accepts_session_grant_access() {
    let payload = r#"{"type":"session.grantAccess","mutationId":"01900000-0000-7000-8000-000000000010","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011","actorUserId":"01900000-0000-7000-8000-000000000012","accessLevel":"suggest","expiresAt":"2026-08-14T00:00:00Z"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("session.grantAccess should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::GrantAccess {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
            actor_user_id,
            access_level,
            expires_at,
        } => {
            assert_eq!(mutation_id, "01900000-0000-7000-8000-000000000010");
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
            assert_eq!(
                expected_runtime_incarnation_id,
                "01900000-0000-7000-8000-000000000011"
            );
            assert_eq!(actor_user_id, "01900000-0000-7000-8000-000000000012");
            assert_eq!(access_level, AccessLevel::Suggest);
            assert_eq!(expires_at, "2026-08-14T00:00:00Z");
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn session_access_grants_event_serializes_camelcase() {
    let event = SessionEvent::AccessGrants {
        session_id: "session-1".to_owned(),
        runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        account_user_id: "user-1".to_owned(),
        grants: vec![AccessGrantEntry {
            actor_user_id: "user-2".to_owned(),
            handle: "alice".to_owned(),
            display_name: "Alice".to_owned(),
            access_level: AccessLevel::Inject,
            granted_at: "2026-01-01T00:00:00Z".to_owned(),
            expires_at: Some("2026-01-02T00:00:00Z".to_owned()),
        }],
    };

    let value = serde_json::to_value(&event).expect("event serializes");
    assert_eq!(value["type"], "session.accessGrants");
    assert_eq!(value["sessionId"], "session-1");
    assert_eq!(
        value["runtimeIncarnationId"],
        "01900000-0000-7000-8000-000000000011"
    );
    assert_eq!(value["accountUserId"], "user-1");
    let grant = &value["grants"][0];
    assert_eq!(grant["actorUserId"], "user-2");
    assert_eq!(grant["displayName"], "Alice");
    assert_eq!(grant["accessLevel"], "inject");
    assert_eq!(grant["grantedAt"], "2026-01-01T00:00:00Z");
}

#[test]
fn accepts_session_interrupt_with_session_id() {
    let payload = r#"{"type":"session.interrupt","requestId":"01900000-0000-7000-8000-000000000020","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011"}"#;
    let envelope: SessionCommand = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(error) => panic!("session.interrupt should deserialize: {error}"),
    };

    match envelope {
        SessionCommand::Interrupt { session_id, .. } => {
            assert_eq!(session_id, "019d1bd1-c0ae-72a0-88cb-c8519739adaa");
        }
        other => panic!("unexpected control command: {other:?}"),
    }
}

#[test]
fn serializes_session_upsert_with_full_row() {
    let payload = serde_json::to_value(SessionEvent::Upsert {
        session: Box::new(sample_runtime_local_session_info()),
    })
    .unwrap_or_else(|error| panic!("session.upsert should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("session.upsert")
    );
    assert_eq!(
        payload
            .get("session")
            .and_then(|session| session.get("id"))
            .and_then(serde_json::Value::as_str),
        Some("session-2")
    );
    assert!(payload.get("session_id").is_none());
}

#[test]
fn session_access_recovery_wire_shapes_are_exact() {
    let command = SessionCommand::AcknowledgeAccessMutation {
        mutation_id: "01900000-0000-7000-8000-000000000010".to_owned(),
        fingerprint: "a".repeat(64),
    };
    command.validate().expect("valid exact acknowledgment");
    let payload = serde_json::to_value(&command).expect("ack should serialize");
    assert_eq!(payload["type"], "session.accessMutationAck");
    assert_eq!(
        payload["mutationId"],
        "01900000-0000-7000-8000-000000000010"
    );
    assert_eq!(payload["fingerprint"], "a".repeat(64));

    let recovered = serde_json::to_value(SessionEvent::AccessMutationRecovered {
        mutation_id: "01900000-0000-7000-8000-000000000010".to_owned(),
        session_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000012".to_owned(),
        originating_account_epoch: 3,
        fingerprint: "b".repeat(64),
        kind: super::SessionAccessMutationKind::Revoke,
        actor_user_id: Some("01900000-0000-7000-8000-000000000013".to_owned()),
        access_level: None,
        expires_at: None,
        outcome: Some(super::SessionAccessMutationOutcome::Applied),
        message: None,
    })
    .expect("recovered event should serialize");
    assert_eq!(recovered["type"], "session.accessMutationRecovered");
    assert_eq!(recovered["originatingAccountEpoch"], 3);
    assert_eq!(recovered["fingerprint"], "b".repeat(64));
    assert_eq!(recovered["outcome"], "applied");
}

#[test]
fn session_access_ack_rejects_non_digest_fingerprint() {
    let command = SessionCommand::AcknowledgeAccessMutation {
        mutation_id: "01900000-0000-7000-8000-000000000010".to_owned(),
        fingerprint: "not-a-digest".to_owned(),
    };
    assert!(command.validate().is_err());
}

#[test]
fn serializes_session_removed_with_session_id() {
    let payload = serde_json::to_value(SessionEvent::Removed {
        session_id: "session-1".to_owned(),
    })
    .unwrap_or_else(|error| panic!("session.removed should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("session.removed")
    );
    assert_eq!(
        payload.get("sessionId").and_then(serde_json::Value::as_str),
        Some("session-1")
    );
    assert!(payload.get("session_id").is_none());
}

#[test]
fn account_context_event_serialization_preserves_signed_out_epoch() {
    let authenticated = serde_json::to_value(AccountSessionEvent::new(
        Some("account-a".to_owned()),
        7,
        SessionEvent::Removed {
            session_id: "session-a".to_owned(),
        },
    ))
    .expect("authenticated envelope should serialize");
    assert_eq!(authenticated["authority"], "accountContext");
    assert_eq!(authenticated["accountUserId"], "account-a");
    assert_eq!(authenticated["accountEpoch"], 7);
    assert_eq!(authenticated["type"], "session.removed");

    let signed_out = serde_json::to_value(AccountSessionEvent::new(
        None,
        8,
        SessionEvent::Removed {
            session_id: "session-b".to_owned(),
        },
    ))
    .expect("signed-out envelope should serialize");
    assert_eq!(signed_out["authority"], "accountContext");
    assert!(signed_out.get("accountUserId").is_none());
    assert_eq!(signed_out["accountEpoch"], 8);
    assert_eq!(signed_out["type"], "session.removed");
}

#[test]
fn serializes_auth_ready_without_token_payload() {
    let payload = serde_json::to_value(AuthEvent::Ready {
        user_id: Some("11111111-1111-1111-1111-111111111111".to_owned()),
        account_epoch: 7,
    })
    .unwrap_or_else(|error| panic!("auth.ready should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("auth.ready")
    );
    assert_eq!(
        payload.get("userId").and_then(serde_json::Value::as_str),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert!(payload.get("token").is_none());
}

#[test]
fn serializes_auth_notice_without_message_when_clear() {
    let payload = serde_json::to_value(AuthEvent::Notice { message: None })
        .unwrap_or_else(|error| panic!("auth.notice should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("auth.notice")
    );
    assert!(payload.get("message").is_none());
}

#[test]
fn serializes_ordered_terminal_product_events() {
    let bell = serde_json::to_value(TerminalEvent::Bell {
        session_id: "session-1".to_owned(),
    })
    .expect("term.bell should serialize");
    let title = serde_json::to_value(TerminalEvent::Title {
        session_id: "session-1".to_owned(),
        title: Some("agent".to_owned()),
    })
    .expect("term.title should serialize");
    let cleared_title = serde_json::to_value(TerminalEvent::Title {
        session_id: "session-1".to_owned(),
        title: None,
    })
    .expect("cleared term.title should serialize");

    assert_eq!(bell["type"], "term.bell");
    assert_eq!(bell["sessionId"], "session-1");
    assert_eq!(title["type"], "term.title");
    assert_eq!(title["sessionId"], "session-1");
    assert_eq!(title["title"], "agent");
    assert!(
        cleared_title
            .get("title")
            .is_some_and(serde_json::Value::is_null)
    );
}

#[test]
fn serializes_correlated_terminal_resize_results() {
    let identity = super::TerminalResizeIdentity {
        request_id: "01900000-0000-7000-8000-000000000020".to_owned(),
        expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000021".to_owned(),
        subscription_id: "subscription-1".to_owned(),
        subscription_generation: 7,
        surface_generation: 3,
        cols: 120,
        rows: 40,
        width_pixels: 1_200,
        height_pixels: 800,
        cell_width_pixels: 10,
        cell_height_pixels: 20,
    };
    let applied = serde_json::to_value(TerminalEvent::ResizeApplied {
        session_id: "session-1".to_owned(),
        identity: identity.clone(),
    })
    .expect("resize applied should serialize");
    let rejected = serde_json::to_value(TerminalEvent::ResizeRejected {
        session_id: "session-1".to_owned(),
        identity: identity.clone(),
        reason: "authority".to_owned(),
    })
    .expect("resize rejected should serialize");

    let expected_identity = serde_json::to_value(identity).expect("identity serializes");
    for payload in [&applied, &rejected] {
        for (key, value) in expected_identity.as_object().expect("identity object") {
            assert_eq!(&payload[key], value, "field {key}");
        }
        assert_eq!(payload["sessionId"], "session-1");
    }
    assert_eq!(applied["type"], "term.resizeApplied");
    assert_eq!(rejected["type"], "term.resizeRejected");
    assert_eq!(rejected["reason"], "authority");
    assert!(applied.get("runtimeIncarnationId").is_none());
}

#[test]
fn serializes_term_notification_with_optional_fields() {
    let payload = serde_json::to_value(TerminalEvent::Notification {
        session_id: "session-1".to_owned(),
        title: Some("Build complete".to_owned()),
        body: Some("All tests passed".to_owned()),
    })
    .unwrap_or_else(|error| panic!("term.notification should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("term.notification")
    );
    assert_eq!(
        payload.get("sessionId").and_then(serde_json::Value::as_str),
        Some("session-1")
    );
    assert_eq!(
        payload.get("title").and_then(serde_json::Value::as_str),
        Some("Build complete")
    );
    assert_eq!(
        payload.get("body").and_then(serde_json::Value::as_str),
        Some("All tests passed")
    );
    assert!(payload.get("session_id").is_none());
}

#[test]
fn serializes_action_result_with_session_context() {
    let payload = serde_json::to_value(SessionEvent::ActionResult {
        session_id: "session-1".to_owned(),
        action_id: "action-1".to_owned(),
        status: RelayActionStatus::Accepted,
    })
    .unwrap_or_else(|error| panic!("action.result should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("action.result")
    );
    assert_eq!(
        payload.get("sessionId").and_then(serde_json::Value::as_str),
        Some("session-1")
    );
    assert_eq!(
        payload.get("actionId").and_then(serde_json::Value::as_str),
        Some("action-1")
    );
    assert_eq!(
        payload.get("status").and_then(serde_json::Value::as_str),
        Some("accepted")
    );
}

#[test]
fn serializes_session_list_with_explicit_remote_shape() {
    let local = SessionListEntry::Local {
        entry: LocalSessionListEntry {
            id: "local-1".to_owned(),
            incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            create_request_id: Some("req-1".to_owned()),
            name: "local-1".to_owned(),
            project: "/Users/john/Repos/kodosi".to_owned(),
            mode: SessionMode::Normal,
            status: RuntimeSessionStatus::Active,
            recovery: kodosi_domain::session::LocalSessionRecoveryState::Live,
            scope: ShareScope::JustMe,
            access: AccessLevel::Inject,
            room_id: None,
            room_name: None,
            active_count: 0,
            entitled_count: 1,
            last_activity: "just now".to_owned().into(),
            semantic_actions: SessionSemanticActions::default(),
            backend_session_id: None,
            backend_incarnation_id: None,
            meta: None,
        },
    };
    let remote = SessionListEntry::Remote {
        entry: RemoteSessionListEntry {
            id: "remote-1".to_owned(),
            incarnation_id: Some("01900000-0000-7000-8000-000000000002".to_owned()),
            name: "remote-1".to_owned(),
            project: "/Users/john/Repos/kodosi".to_owned(),
            mode: SessionMode::Normal,
            status: RuntimeSessionStatus::Active,
            scope: ShareScope::Friends,
            access: AccessLevel::View,
            owner: Some("alice".to_owned()),
            owner_user_id: Some("user-alice".to_owned()),
            permissions: PermissionFlags(0x01),
            room_id: Some("room-1".to_owned()),
            room_name: Some("Acme".to_owned()),
            connection_state: Some(ConnectionState::Offline),
            connection_reason: Some("Access revoked".to_owned()),
            access_state: Some(RemoteSessionAccessState::AccessDenied),
            access_reason: Some("Access revoked".to_owned()),
            access_issue: Some(RemoteSessionAccessIssue::PeerIdentityChanged),
            active_count: 3,
            entitled_count: 9,
            last_activity: "just now".to_owned().into(),
            semantic_actions: SessionSemanticActions::default(),
        },
    };

    let payload = serde_json::to_value(SessionEvent::List {
        sessions: vec![local, remote],
    })
    .unwrap_or_else(|error| panic!("session.list should serialize: {error}"));
    insta::assert_json_snapshot!("session_list_explicit_remote_shape", payload);
}

#[test]
fn omits_optional_runtime_meta_fields_when_absent() {
    let info = SessionListEntry::Local {
        entry: LocalSessionListEntry {
            id: "local-1".to_owned(),
            incarnation_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            create_request_id: None,
            name: "local-1".to_owned(),
            project: "/Users/john/Repos/kodosi".to_owned(),
            mode: SessionMode::Normal,
            status: RuntimeSessionStatus::Active,
            recovery: kodosi_domain::session::LocalSessionRecoveryState::Live,
            scope: ShareScope::JustMe,
            access: AccessLevel::Inject,
            room_id: None,
            room_name: None,
            active_count: 0,
            entitled_count: 1,
            last_activity: "just now".to_owned().into(),
            semantic_actions: SessionSemanticActions::default(),
            backend_session_id: None,
            backend_incarnation_id: None,
            meta: Some(Box::new(SessionListEntryMeta {
                agent: "terminal".to_owned(),
                working_dir: "/Users/john/Repos/kodosi".to_owned(),
                token_percent: 0,
                git_repo: None,
                git_url: None,
                git_branch: None,
                git_remotes: Vec::new(),
                project_type: None,
                package_manager: None,
                manifest_files: Vec::new(),
                running_command: None,
                detected_agent: None,
                terminal_title: None,
            })),
        },
    };

    let payload = serde_json::to_value(SessionEvent::List {
        sessions: vec![info],
    })
    .unwrap_or_else(|error| panic!("session.list should serialize: {error}"));
    let session = payload
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .and_then(|sessions| sessions.first())
        .unwrap_or_else(|| panic!("session.list payload should include one session"));
    let meta = session
        .get("meta")
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("runtime session should include meta object"));

    assert!(
        !meta.contains_key("gitRepo"),
        "gitRepo should be omitted when absent"
    );
    assert!(
        !meta.contains_key("gitUrl"),
        "gitUrl should be omitted when absent"
    );
    assert_eq!(
        meta.get("gitRemotes")
            .and_then(serde_json::Value::as_array)
            .map(Vec::is_empty),
        Some(true),
        "required gitRemotes should serialize as an empty array"
    );
    assert_eq!(
        meta.get("manifestFiles")
            .and_then(serde_json::Value::as_array)
            .map(Vec::is_empty),
        Some(true),
        "required manifestFiles should serialize as an empty array"
    );
}

#[test]
fn serializes_room_list_with_camel_case_fields() {
    let payload = serde_json::to_value(SessionEvent::RoomList {
        rooms: vec![RoomListEntry {
            id: "room-1".to_owned(),
            name: "Acme".to_owned(),
            slug: "acme".to_owned(),
        }],
    })
    .unwrap_or_else(|error| panic!("room.list should serialize: {error}"));

    assert_eq!(
        payload.get("type").and_then(serde_json::Value::as_str),
        Some("room.list")
    );
    let rooms = payload
        .get("rooms")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("room.list payload should include rooms array"));
    assert_eq!(rooms.len(), 1);
    assert_eq!(
        rooms[0].get("slug").and_then(serde_json::Value::as_str),
        Some("acme")
    );
}

#[test]
fn rejects_session_create_without_request_id() {
    let payload = r#"{"type":"session.create","name":"session-123","workingDir":"/tmp/project"}"#;
    let result = serde_json::from_str::<SessionCommand>(payload);
    assert!(
        result.is_err(),
        "session.create without requestId must fail"
    );
}

#[test]
fn accepts_focus_and_blur_messages() {
    let focus_payload = r#"{"type":"session.focus","sessionId":"session-1","clientId":"client-a","requestId":"01900000-0000-7000-8000-000000000002","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000001"}"#;
    let blur_payload = r#"{"type":"session.blur","sessionId":"session-1","clientId":"client-a","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000001"}"#;

    let focus_envelope: TerminalCommand = serde_json::from_str(focus_payload)
        .unwrap_or_else(|error| panic!("session.focus should deserialize: {error}"));
    let blur_envelope: TerminalCommand = serde_json::from_str(blur_payload)
        .unwrap_or_else(|error| panic!("session.blur should deserialize: {error}"));

    match focus_envelope {
        TerminalCommand::Focus {
            session_id,
            client_id,
            request_id,
            expected_runtime_incarnation_id,
        } => {
            assert_eq!(session_id, "session-1");
            assert_eq!(client_id, "client-a");
            assert_eq!(request_id, "01900000-0000-7000-8000-000000000002");
            assert_eq!(
                expected_runtime_incarnation_id,
                "01900000-0000-7000-8000-000000000001"
            );
        }
        other => panic!("unexpected focus control command: {other:?}"),
    }

    match blur_envelope {
        TerminalCommand::Blur {
            session_id,
            client_id,
            expected_runtime_incarnation_id,
            ..
        } => {
            assert_eq!(session_id, "session-1");
            assert_eq!(client_id, "client-a");
            assert_eq!(
                expected_runtime_incarnation_id,
                "01900000-0000-7000-8000-000000000001"
            );
        }
        other => panic!("unexpected blur control command: {other:?}"),
    }
}

#[test]
fn accepts_session_delete_message() {
    let payload = r#"{"type":"session.delete","requestId":"01900000-0000-7000-8000-000000000021","sessionId":"session-1","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011"}"#;
    let envelope: SessionCommand = serde_json::from_str(payload)
        .unwrap_or_else(|error| panic!("session.delete should deserialize: {error}"));

    match envelope {
        SessionCommand::Delete { session_id, .. } => assert_eq!(session_id, "session-1"),
        other => panic!("unexpected delete control command: {other:?}"),
    }
}

#[test]
fn accepts_session_close_message() {
    let payload = r#"{"type":"session.close","requestId":"01900000-0000-7000-8000-000000000021","sessionId":"session-1","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011"}"#;
    let envelope: SessionCommand = serde_json::from_str(payload)
        .unwrap_or_else(|error| panic!("session.close should deserialize: {error}"));

    match envelope {
        SessionCommand::Close { session_id, .. } => assert_eq!(session_id, "session-1"),
        other => panic!("unexpected close control command: {other:?}"),
    }
}

#[test]
fn accepts_session_reopen_message() {
    let payload = r#"{"type":"session.reopen","requestId":"01900000-0000-7000-8000-000000000022","sessionId":"session-1","expectedRuntimeIncarnationId":"01900000-0000-7000-8000-000000000011"}"#;
    let envelope: SessionCommand = serde_json::from_str(payload)
        .unwrap_or_else(|error| panic!("session.reopen should deserialize: {error}"));

    match envelope {
        SessionCommand::Reopen { session_id, .. } => assert_eq!(session_id, "session-1"),
        other => panic!("unexpected reopen control command: {other:?}"),
    }
}

#[test]
fn rejects_unknown_fields_in_host_messages() {
    let payload = r#"{"type":"session.stop","sessionId":"019d1bd1-c0ae-72a0-88cb-c8519739adaa","unexpectedField":true}"#;
    let result = serde_json::from_str::<SessionCommand>(payload);
    assert!(
        result.is_err(),
        "control message with unknown fields must fail deserialization"
    );
}

#[test]
fn desktop_runtime_authority_json_matches_generated_contract() {
    let generated = render_desktop_runtime_authority_json()
        .unwrap_or_else(|error| panic!("desktop-runtime authority JSON should render: {error}"));
    assert_eq!(
        generated,
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../protocol/desktop-runtime-authority.json"
        ))
    );
}

#[test]
fn semantic_command_authority_pins_present_and_absent_query_filter() {
    let generated = render_desktop_runtime_authority_json()
        .unwrap_or_else(|error| panic!("desktop-runtime authority JSON should render: {error}"));
    let authority: serde_json::Value =
        serde_json::from_str(&generated).expect("generated authority should parse");

    assert_eq!(
        authority["semanticSendModes"],
        serde_json::json!(["queue", "steer", "stopAndSend"])
    );

    let semantic_send = &authority["messageShapes"]["agent.intel.semanticSend"];
    assert_eq!(semantic_send["optionalFields"], serde_json::json!([]));
    assert_eq!(
        semantic_send["requiredFields"],
        serde_json::json!([
            "incarnationId",
            "mode",
            "requestId",
            "sessionId",
            "text",
            "type"
        ])
    );

    let query = &authority["messageShapes"]["agent.intel.querySteer"];
    assert_eq!(
        query["requiredFields"],
        serde_json::json!(["requestId", "sessionId", "type"])
    );
    assert_eq!(
        query["optionalFields"],
        serde_json::json!(["semanticRequestId"])
    );
    assert_eq!(
        query["nullableFields"],
        serde_json::json!(["semanticRequestId"])
    );
}

#[test]
fn room_chat_authority_pins_field_presence_contract() {
    let generated = render_desktop_runtime_authority_json()
        .unwrap_or_else(|error| panic!("desktop-runtime authority JSON should render: {error}"));
    let authority: serde_json::Value =
        serde_json::from_str(&generated).expect("generated authority should parse");
    let chat_post = &authority["messageShapes"]["room.chat.post"];

    assert_eq!(
        chat_post["requiredFields"],
        serde_json::json!(["body", "room_id", "type"])
    );
    assert_eq!(
        chat_post["optionalFields"],
        serde_json::json!([
            "author_session_id",
            "recipient_session_ids",
            "recipient_user_ids",
            "requestId"
        ])
    );
    assert_eq!(
        chat_post["nullableFields"],
        serde_json::json!(["author_session_id", "requestId"])
    );

    let posted_message = &authority["messageShapes"]["room.chat.posted"]["object"]["message"];
    assert!(
        posted_message["optionalFields"]
            .as_array()
            .is_some_and(|fields| {
                fields.contains(&serde_json::json!("recipientSessionIds"))
                    && fields.contains(&serde_json::json!("recipientUserIds"))
            }),
        "room chat recipient arrays must remain optional for older payloads"
    );
}

#[test]
fn room_action_status_wire_vocabulary_is_stable() {
    use crate::host_protocol::RoomActionStatus;

    assert_eq!(
        serde_json::to_value(RoomActionStatus::Succeeded).expect("status"),
        "succeeded"
    );
    assert_eq!(
        serde_json::to_value(RoomActionStatus::Failed).expect("status"),
        "failed"
    );
}

#[test]
fn hidden_session_command_and_event_shapes_are_stable() {
    use crate::{HiddenSessionEntry, SessionCommand, SessionEvent};

    let command = serde_json::to_value(SessionCommand::ListHidden {
        request_id: "req-hidden".to_owned(),
    })
    .expect("list hidden command");
    assert_eq!(
        command,
        serde_json::json!({
            "type": "session.listHidden",
            "requestId": "req-hidden",
        })
    );

    let event = serde_json::to_value(SessionEvent::HiddenList {
        request_id: "req-hidden".to_owned(),
        entries: vec![HiddenSessionEntry {
            id: "session-1".to_owned(),
            name: "Agent".to_owned(),
            project: "/repo".to_owned(),
            owner: "Alice".to_owned(),
        }],
    })
    .expect("hidden list event");
    assert_eq!(
        event,
        serde_json::json!({
            "type": "session.hiddenList",
            "requestId": "req-hidden",
            "entries": [{
                "id": "session-1",
                "name": "Agent",
                "project": "/repo",
                "owner": "Alice",
            }],
        })
    );
}

#[test]
fn share_scope_wire_strings_are_stable_camelcase() {
    use kodosi_domain::permissions::ShareScope;
    let cases = [
        (ShareScope::JustMe, "justMe"),
        (ShareScope::MyDevices, "myDevices"),
        (ShareScope::Friends, "friends"),
        (ShareScope::Room, "room"),
    ];
    for (scope, wire) in cases {
        let value = serde_json::to_value(scope).expect("ShareScope must serialize");
        assert_eq!(
            value,
            serde_json::Value::String(wire.to_owned()),
            "scope {scope:?} must serialize as {wire:?}"
        );
    }
}
