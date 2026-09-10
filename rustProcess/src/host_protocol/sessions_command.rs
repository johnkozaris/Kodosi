use crate::{HiddenSessionEntry, Result, SessionCommand, SessionEvent, runtime::Runtime};
use kodosi_domain::ids::SessionId;
use uuid::Uuid;

#[derive(Debug, Default)]
pub(crate) struct SessionCommandEffects {
    pub(crate) force_snapshot: bool,
    pub(crate) defer_catalog_replay: bool,
    pub(crate) events: Vec<SessionEvent>,
}

#[expect(
    clippy::too_many_lines,
    reason = "Session command routing stays in one match so the typed IPC surface is auditable."
)]
pub(crate) async fn apply_session_command(
    app: &mut Runtime,
    command: SessionCommand,
) -> Result<SessionCommandEffects> {
    let mut effects = SessionCommandEffects::default();

    match command {
        SessionCommand::Create {
            request_id,
            name,
            working_dir,
            resume,
        } => {
            tracing::info!(
                request_id = %request_id,
                requested_name = %name,
                working_dir = ?working_dir,
                "received session.create"
            );
            effects.force_snapshot = crate::runtime::local_sessions::lifecycle(app)
                .create(Some(name), working_dir, Some(request_id.clone()), resume)
                .await?;
            let created = app
                .state
                .local
                .sessions
                .ids()
                .iter()
                .filter_map(|id| app.state.local.sessions.record(*id))
                .find(|record| {
                    record.launch_committed
                        && record.create_request_id.as_deref() == Some(request_id.as_str())
                })
                .ok_or_else(|| crate::AppError::Unsupported {
                    reason: "created session receipt is unavailable".to_owned(),
                })?;
            effects.events.push(SessionEvent::Created {
                request_id,
                session_id: created.summary.id.to_string(),
                runtime_incarnation_id: created.local_incarnation_id.to_string(),
            });
        }
        SessionCommand::Rename { session_id, name } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            tracing::debug!(session_id = %session_id, "received session.rename");
            app.rename_session(session_id, &name).await?;
        }
        SessionCommand::SetMode {
            session_id,
            expected_runtime_incarnation_id,
            mode,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            validate_runtime_incarnation(app, session_id, expected)?;
            tracing::debug!(session_id = %session_id, ?mode, "received session.mode");
            app.set_session_mode(session_id, expected, mode)?;
        }
        SessionCommand::Stop {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            validate_runtime_incarnation(app, session_id, expected)?;
            tracing::debug!(session_id = %session_id, %request_id, "received session.stop");
            app.stop_session(session_id).await?;
        }
        SessionCommand::Close {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            validate_runtime_incarnation(app, session_id, expected)?;
            reject_remote_local_lifecycle_command(app, session_id, "close")?;
            tracing::debug!(session_id = %session_id, %request_id, "received session.close");
            app.stop_session(session_id).await?;
            crate::runtime::local_sessions::lifecycle(app).delete(session_id)?;
        }
        SessionCommand::Interrupt {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            validate_runtime_incarnation(app, session_id, expected)?;
            tracing::debug!(session_id = %session_id, %request_id, "received session.interrupt");
            app.interrupt_session(session_id).await?;
            effects.events.push(SessionEvent::Interrupted {
                request_id,
                session_id: session_id.to_string(),
                runtime_incarnation_id: expected.to_string(),
            });
        }
        SessionCommand::Delete {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            validate_runtime_incarnation(app, session_id, expected)?;
            tracing::debug!(session_id = %session_id, %request_id, "received session.delete");
            reject_remote_local_lifecycle_command(app, session_id, "delete")?;
            crate::runtime::local_sessions::lifecycle(app).delete(session_id)?;
            app.cancel_relay_prepare_for_session(session_id);
            app.cancel_share_transition_for_session(
                session_id,
                "session deleted during share transition",
            );
        }
        SessionCommand::Reopen {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            validate_runtime_incarnation(app, session_id, expected)?;
            tracing::debug!(session_id = %session_id, %request_id, "received session.reopen");
            reject_remote_local_lifecycle_command(app, session_id, "reopen")?;
            let reopened = crate::runtime::local_sessions::lifecycle(app)
                .reopen(session_id)
                .await?;
            if reopened {
                app.cancel_relay_prepare_for_session(session_id);
                app.cancel_share_transition_for_session(
                    session_id,
                    "session reopened during share transition",
                );
            }
        }
        SessionCommand::OpenRemote { session_id } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            tracing::debug!(session_id = %session_id, "received session.openRemote");
            crate::runtime::remote_sessions::open(app, session_id).await?;
            effects.events.push(SessionEvent::Opened {
                session_id: session_id.to_string(),
            });
            effects.force_snapshot = true;
        }
        SessionCommand::Hide { session_id } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            tracing::debug!(session_id = %session_id, "received session.hide");
            if crate::runtime::remote_sessions::hide(app, session_id).await? {
                effects.force_snapshot = true;
            }
        }
        SessionCommand::Leave {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let mutation_id = parse_uuid(&mutation_id, "mutationId")?;
            let expected_runtime_incarnation_id = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            tracing::debug!(session_id = %session_id, mutation_id = %mutation_id, "received session.leave");
            validate_runtime_incarnation(app, session_id, expected_runtime_incarnation_id)?;
            crate::runtime::remote_sessions::leave(
                app,
                session_id,
                expected_runtime_incarnation_id,
                mutation_id,
            )?;
        }
        SessionCommand::Unhide { session_id } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            tracing::debug!(session_id = %session_id, "received session.unhide");
            if crate::runtime::remote_sessions::unhide(app, session_id)? {
                effects.force_snapshot = true;
            }
        }
        SessionCommand::ListHidden { request_id } => {
            let entries = app
                .state
                .discovery
                .hidden_sessions()
                .map(|record| HiddenSessionEntry {
                    id: record.summary.id.to_string(),
                    name: record.summary.title.clone(),
                    project: record
                        .summary
                        .working_dir
                        .clone()
                        .or_else(|| record.summary.room_name.clone())
                        .unwrap_or_else(|| "Remote session".to_owned()),
                    owner: record.summary.owner_name.clone(),
                })
                .collect();
            effects.events.push(SessionEvent::HiddenList {
                request_id,
                entries,
            });
        }
        SessionCommand::SetShareScope {
            request_id,
            session_id,
            expected_runtime_incarnation_id,
            scope,
            room_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected_runtime_incarnation_id = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            tracing::debug!(session_id = %session_id, ?scope, "received session.scope");
            validate_runtime_incarnation(app, session_id, expected_runtime_incarnation_id)?;
            let transition_id = parse_uuid(&request_id, "requestId")?;
            app.begin_share_transition(
                transition_id,
                session_id,
                expected_runtime_incarnation_id,
                scope,
                room_id.as_deref(),
            )?;
            effects.force_snapshot = true;
            return Ok(effects);
        }
        SessionCommand::GrantAccess {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
            actor_user_id,
            access_level,
            expires_at,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let mutation_id = parse_uuid(&mutation_id, "mutationId")?;
            let expected_runtime_incarnation_id = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            tracing::debug!(session_id = %session_id, mutation_id = %mutation_id, %actor_user_id, "received session.grantAccess");
            validate_runtime_incarnation(app, session_id, expected_runtime_incarnation_id)?;
            crate::runtime::sharing::grant_access(
                app,
                session_id,
                expected_runtime_incarnation_id,
                mutation_id,
                &actor_user_id,
                access_level,
                &expires_at,
            )?;
        }
        SessionCommand::RevokeAccess {
            mutation_id,
            session_id,
            expected_runtime_incarnation_id,
            actor_user_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let mutation_id = parse_uuid(&mutation_id, "mutationId")?;
            let expected_runtime_incarnation_id = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            tracing::debug!(session_id = %session_id, mutation_id = %mutation_id, %actor_user_id, "received session.revokeAccess");
            validate_runtime_incarnation(app, session_id, expected_runtime_incarnation_id)?;
            crate::runtime::sharing::revoke_access(
                app,
                session_id,
                expected_runtime_incarnation_id,
                mutation_id,
                &actor_user_id,
            )?;
        }
        SessionCommand::ListAccess {
            session_id,
            expected_runtime_incarnation_id,
        } => {
            let session_id = SessionId::parse_field(&session_id, "sessionId")?;
            let expected = parse_uuid(
                &expected_runtime_incarnation_id,
                "expectedRuntimeIncarnationId",
            )?;
            tracing::debug!(session_id = %session_id, "received session.listAccess");
            let (runtime_incarnation, grants) =
                crate::runtime::sharing::list_access(app, session_id, expected).await?;
            effects.events.push(SessionEvent::AccessGrants {
                session_id: session_id.to_string(),
                runtime_incarnation_id: runtime_incarnation.to_string(),
                account_user_id: authenticated_account_user_id(app)?,
                grants,
            });
        }
        SessionCommand::RecoverAccessMutations => {
            let account = authenticated_account_user_id(app)?;
            for mutation in app.access_mutations.entries(&account)? {
                effects
                    .events
                    .push(crate::runtime::sharing::recovered_access_mutation_event(
                        mutation.clone(),
                    ));
                if !matches!(
                    mutation.state,
                    crate::runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(
                        _
                    )
                ) {
                    crate::runtime::sharing::queue_access_mutation_reconciliation(app, &mutation);
                }
            }
        }
        SessionCommand::ReconcileAccessMutation { mutation_id } => {
            let mutation_id = parse_uuid(&mutation_id, "mutationId")?;
            let account = authenticated_account_user_id(app)?;
            let mutation = app.access_mutations.get(&account, mutation_id)?.cloned();
            effects.events.push(SessionEvent::AccessMutationReconciled {
                mutation_id: mutation_id.to_string(),
                present: mutation.is_some(),
            });
            if let Some(mutation) = mutation.as_ref() {
                effects
                    .events
                    .push(crate::runtime::sharing::recovered_access_mutation_event(
                        mutation.clone(),
                    ));
                if !matches!(
                    mutation.state,
                    crate::runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(
                        _
                    )
                ) {
                    crate::runtime::sharing::queue_access_mutation_reconciliation(app, mutation);
                }
            }
        }
        SessionCommand::AcknowledgeAccessMutation {
            mutation_id,
            fingerprint,
        } => {
            let removed = crate::runtime::sharing::acknowledge_access_mutation(
                app,
                parse_uuid(&mutation_id, "mutationId")?,
                &fingerprint,
            )?;
            effects.events.push(if removed {
                SessionEvent::AccessMutationAcknowledged {
                    mutation_id,
                    fingerprint,
                }
            } else {
                SessionEvent::AccessMutationReconciled {
                    mutation_id,
                    present: false,
                }
            });
        }
        SessionCommand::SnapshotRefresh => {
            tracing::debug!("received session.list; deferring state replay to next cycle");
            effects.defer_catalog_replay = true;
        }
    }

    Ok(effects)
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| crate::AppError::InvalidBackendData {
        field: field.to_owned(),
        reason: error.to_string(),
    })
}

fn validate_runtime_incarnation(
    app: &Runtime,
    session_id: SessionId,
    expected_runtime_incarnation_id: Uuid,
) -> Result<()> {
    let actual = app
        .state
        .local
        .sessions
        .record(session_id)
        .map(|record| record.local_incarnation_id)
        .or_else(|| {
            app.state
                .discovery
                .session(session_id)
                .and_then(|record| record.incarnation_id)
        })
        .ok_or(crate::AppError::NoActiveSession)?;
    if actual != expected_runtime_incarnation_id {
        return Err(crate::AppError::NoActiveSession);
    }
    Ok(())
}

fn authenticated_account_user_id(app: &Runtime) -> Result<String> {
    app.state
        .identity
        .auth
        .subject_string()
        .ok_or(crate::AppError::Unauthorized)
}

fn reject_remote_local_lifecycle_command(
    app: &Runtime,
    session_id: SessionId,
    operation: &str,
) -> Result<()> {
    if app.state.discovery.session(session_id).is_some() {
        return Err(crate::AppError::Unsupported {
            reason: format!(
                "cannot {operation} remote session {session_id} from this runtime; use session.hide locally or run the command on its owner device"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio_util::sync::CancellationToken;

    use super::apply_session_command;
    use crate::{
        SessionCommand, SessionEvent, config::AppConfig, discovery::RemoteSessionRecord,
        runtime::Runtime,
    };
    use kodosi_domain::{
        auth::AuthState,
        ids::{SessionId, UserId},
        permissions::{AccessLevel, ShareScope},
        session::{SessionMode, SessionRole, SessionSummary},
        terminal::TerminalSize,
    };

    fn test_app() -> Runtime {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap_or_else(|error| panic!("test app should construct: {error}"))
    }

    fn test_app_with_backend() -> Runtime {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        config.backend.api = Some("http://127.0.0.1".to_owned());
        config.backend.viewer_relay = Some("ws://127.0.0.1/viewer".to_owned());
        Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap_or_else(|error| panic!("test app should construct: {error}"))
    }

    fn test_app_with_api(api: String) -> Runtime {
        let mut config = AppConfig::default();
        config.auth.keyring_service = "kodosi.test".to_owned();
        config.backend.api = Some(api);
        Runtime::with_dependencies(
            config.clone(),
            CancellationToken::new(),
            crate::runtime::RuntimeDependencies::isolated(&config)
                .expect("isolated runtime dependencies"),
        )
        .unwrap_or_else(|error| panic!("test app should construct: {error}"))
    }

    async fn no_content_server() -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.expect("read request");
            socket
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("write response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    #[test]
    fn session_scope_command_exposes_metadata() {
        let session_id = "019d1bd1-c0ae-72a0-88cb-c8519739adaa".to_owned();
        let command = SessionCommand::SetShareScope {
            request_id: "req-scope".to_owned(),
            session_id: session_id.clone(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
            scope: ShareScope::Room,
            room_id: Some("room-1".to_owned()),
        };

        assert_eq!(command.operation(), "session.scope");
        assert_eq!(command.request_id(), Some("req-scope"));
        assert_eq!(command.session_id(), Some(session_id.as_str()));
    }

    #[test]
    fn session_leave_command_exposes_metadata() {
        let session_id = "019d1bd1-c0ae-72a0-88cb-c8519739adaa".to_owned();
        let command = SessionCommand::Leave {
            mutation_id: "01900000-0000-7000-8000-000000000010".to_owned(),
            session_id: session_id.clone(),
            expected_runtime_incarnation_id: "01900000-0000-7000-8000-000000000011".to_owned(),
        };

        assert_eq!(command.operation(), "session.leave");
        assert_eq!(command.session_id(), Some(session_id.as_str()));
    }

    #[test]
    fn session_close_command_exposes_exact_request_and_target() {
        let request_id = uuid::Uuid::now_v7().to_string();
        let session_id = SessionId::new().to_string();
        let command = SessionCommand::Close {
            request_id: request_id.clone(),
            session_id: session_id.clone(),
            expected_runtime_incarnation_id: uuid::Uuid::now_v7().to_string(),
        };

        assert_eq!(command.operation(), "session.close");
        assert_eq!(command.request_id(), Some(request_id.as_str()));
        assert_eq!(command.session_id(), Some(session_id.as_str()));
        command.validate().expect("close command should validate");
    }

    #[test]
    fn lifecycle_commands_expose_request_ids() {
        let request_id = uuid::Uuid::now_v7().to_string();
        let session_id = SessionId::new().to_string();
        let incarnation_id = uuid::Uuid::now_v7().to_string();
        let commands = [
            SessionCommand::Stop {
                session_id: session_id.clone(),
                expected_runtime_incarnation_id: incarnation_id.clone(),
                request_id: request_id.clone(),
            },
            SessionCommand::Interrupt {
                session_id: session_id.clone(),
                expected_runtime_incarnation_id: incarnation_id.clone(),
                request_id: request_id.clone(),
            },
            SessionCommand::Delete {
                session_id: session_id.clone(),
                expected_runtime_incarnation_id: incarnation_id.clone(),
                request_id: request_id.clone(),
            },
            SessionCommand::Reopen {
                session_id,
                expected_runtime_incarnation_id: incarnation_id,
                request_id: request_id.clone(),
            },
        ];

        for command in commands {
            assert_eq!(command.request_id(), Some(request_id.as_str()));
        }
    }

    #[tokio::test]
    async fn stale_mode_incarnation_is_rejected_before_mutation() {
        let mut app = test_app();
        let session_id = SessionId::new();
        let current_incarnation = uuid::Uuid::now_v7();
        let mut summary = SessionSummary::new_remote(
            session_id,
            "Owned elsewhere".to_owned(),
            "You".to_owned(),
            None,
            ShareScope::Friends,
            AccessLevel::Inject,
            TerminalSize::default(),
        );
        summary.role = SessionRole::Owner;
        summary.detected_agent = Some("Claude".to_owned());
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary,
                incarnation_id: Some(current_incarnation),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );

        let error = apply_session_command(
            &mut app,
            SessionCommand::SetMode {
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: uuid::Uuid::now_v7().to_string(),
                mode: SessionMode::Plan,
            },
        )
        .await
        .expect_err("stale mode command must fail closed");

        assert!(matches!(error, crate::AppError::NoActiveSession));
        let unsupported = apply_session_command(
            &mut app,
            SessionCommand::SetMode {
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: current_incarnation.to_string(),
                mode: SessionMode::Plan,
            },
        )
        .await
        .expect_err("keystrokes must not be presented as confirmed mode control");
        assert!(matches!(unsupported, crate::AppError::Unsupported { .. }));
        assert_eq!(
            app.state
                .discovery
                .session(session_id)
                .expect("record remains")
                .summary
                .mode,
            SessionMode::Normal
        );
    }

    #[tokio::test]
    async fn open_remote_session_keeps_same_id_durable_local_authority() {
        let mut app = test_app_with_backend();
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user")),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.set_remote_surfaces_ready_for_test();
        let session_id = SessionId::new();
        let mut local = SessionSummary::new_owned(
            session_id,
            "Local".to_owned(),
            "shell".to_owned(),
            TerminalSize::new(80, 24).expect("size"),
            None,
        );
        local.state = kodosi_domain::session::SessionState::Stopped;
        app.state.local.sessions.insert(local);

        apply_session_command(
            &mut app,
            SessionCommand::OpenRemote {
                session_id: session_id.to_string(),
            },
        )
        .await
        .expect("same-id backend projection must resolve to retained local authority");

        assert_eq!(
            app.state.shelf.active_session(),
            Some(crate::discovery::ShelfItem::Owned(session_id))
        );
        assert!(!app.state.remote.session_relays.contains(session_id));
    }

    #[tokio::test]
    async fn open_remote_session_rejects_missing_discovery_record() {
        let mut app = test_app_with_backend();
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from("01900000-0000-7000-8000-000000000001").expect("user")),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.set_remote_surfaces_ready_for_test();
        let session_id = SessionId::new();

        let error = apply_session_command(
            &mut app,
            SessionCommand::OpenRemote {
                session_id: session_id.to_string(),
            },
        )
        .await
        .expect_err("missing remote discovery record should fail");

        assert!(
            error
                .to_string()
                .contains("is not available in the current discovery catalog"),
            "unexpected error: {error}"
        );
        assert_eq!(app.state.shelf.active_session(), None);
    }

    #[tokio::test]
    async fn participant_leave_dispatches_and_removes_remote_session() {
        let (api, server) = no_content_server().await;
        let mut app = test_app_with_api(api);
        let account = "01900000-0000-7000-8000-000000000001";
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(account).expect("user")),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(7));
        app.backend
            .set_access_token(Some("test-token".to_owned().into()));
        app.set_remote_surfaces_ready_for_test();
        let session_id = SessionId::new();
        let incarnation_id = uuid::Uuid::now_v7();
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary: SessionSummary::new_remote(
                    session_id,
                    "Remote".to_owned(),
                    "Owner".to_owned(),
                    None,
                    ShareScope::Room,
                    AccessLevel::View,
                    TerminalSize::default(),
                ),
                incarnation_id: Some(incarnation_id),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );
        let checkpoint = kodosi_domain::terminal::TerminalCheckpointV2::new(
            TerminalSize::default(),
            kodosi_domain::terminal::TerminalScreen::Primary,
            br#"{"schemaVersion":1}"#.to_vec(),
            0,
            0,
            false,
        )
        .expect("checkpoint");
        app.remote_terminal
            .install_checkpoint(session_id, checkpoint.clone(), 4);
        app.terminal_hub
            .install_semantic_checkpoint(session_id, checkpoint, 4);
        let _terminal = app
            .terminal_hub
            .register(
                session_id,
                crate::terminal_transport::TerminalSurface::Desktop,
                crate::terminal_transport::TerminalCapability::Write,
            )
            .expect("session should be live");
        let _ = app
            .client_focus
            .note_focus(session_id, "desktop".to_owned());
        let mutation_id = uuid::Uuid::now_v7();

        apply_session_command(
            &mut app,
            SessionCommand::Leave {
                mutation_id: mutation_id.to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
        )
        .await
        .expect("leave admission");
        app.run_periodic_maintenance_force().await;
        let request = server.await.expect("leave request");
        for _ in 0..100 {
            if app.access_mutation_worker_finished_for_test() {
                break;
            }
            tokio::task::yield_now().await;
        }
        app.run_periodic_maintenance_force().await;

        assert!(request.starts_with(&format!(
            "DELETE /api/sessions/{session_id}/access/me?expectedIncarnationId={incarnation_id}&mutationId={mutation_id} "
        )));
        assert!(app.state.discovery.session(session_id).is_none());
        assert!(app.remote_terminal.cached(session_id).is_none());
        assert!(app.client_focus.clients(session_id).is_empty());
        assert_eq!(app.terminal_hub.connection_count(session_id), 0);
        assert!(matches!(
            app.access_mutations
                .get(account, mutation_id)
                .expect("ledger")
                .expect("leave result")
                .state,
            crate::runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(
                crate::runtime::access_mutations::SessionAccessMutationTerminal {
                    status: crate::runtime::access_mutations::SessionAccessMutationTerminalStatus::Applied,
                    ..
                }
            )
        ));
    }

    #[tokio::test]
    async fn proven_leave_does_not_remove_replacement_incarnation() {
        let mut app = test_app();
        let account = "01900000-0000-7000-8000-000000000001";
        app.state.identity.auth = AuthState::Authenticated {
            subject: Some(UserId::try_from(account).expect("user")),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        app.state
            .identity
            .set_account_epoch_for_test(crate::session_runtime::events::AccountEpoch::for_test(7));
        let session_id = SessionId::new();
        let old_incarnation = uuid::Uuid::now_v7();
        let replacement_incarnation = uuid::Uuid::now_v7();
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary: SessionSummary::new_remote(
                    session_id,
                    "Replacement".to_owned(),
                    "Owner".to_owned(),
                    None,
                    ShareScope::Room,
                    AccessLevel::View,
                    TerminalSize::default(),
                ),
                incarnation_id: Some(replacement_incarnation),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );
        let mut prepared = crate::runtime::access_mutations::PreparedSessionAccessMutation::new(
            uuid::Uuid::now_v7(),
            account.to_owned(),
            7,
            session_id,
            old_incarnation,
            session_id.to_string(),
            old_incarnation,
            crate::runtime::access_mutations::SessionAccessMutationTarget::Leave,
        )
        .expect("leave");
        prepared.state =
            crate::runtime::access_mutations::PreparedSessionAccessMutationState::Attempting;
        app.access_mutations
            .put(prepared.clone())
            .expect("persist leave");

        app.apply_access_mutation_completion_for_test(
            crate::runtime::access_mutation_worker::SessionAccessMutationWorkerCompletion {
                prepared: prepared.clone(),
                account_epoch: 7,
                mode: crate::runtime::access_mutation_worker::SessionAccessMutationWorkerMode::Reconcile,
                outcome:
                    crate::runtime::access_mutation_worker::SessionAccessMutationWorkerOutcome::Applied,
                access_snapshot: None,
            },
        )
        .await;

        assert_eq!(
            app.state
                .discovery
                .session(session_id)
                .and_then(|record| record.incarnation_id),
            Some(replacement_incarnation)
        );
        assert!(matches!(
            app.access_mutations
                .get(account, prepared.mutation_id)
                .expect("ledger")
                .expect("terminal leave")
                .state,
            crate::runtime::access_mutations::PreparedSessionAccessMutationState::Terminal(
                crate::runtime::access_mutations::SessionAccessMutationTerminal {
                    status: crate::runtime::access_mutations::SessionAccessMutationTerminalStatus::Applied,
                    ..
                }
            )
        ));
    }

    #[tokio::test]
    async fn owner_cannot_leave_their_own_remote_session() {
        let mut app = test_app();
        let session_id = SessionId::new();
        let mut summary = SessionSummary::new_remote(
            session_id,
            "Owned elsewhere".to_owned(),
            "You".to_owned(),
            None,
            ShareScope::Friends,
            AccessLevel::Inject,
            TerminalSize::default(),
        );
        summary.role = SessionRole::Owner;
        let incarnation_id = uuid::Uuid::now_v7();
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary,
                incarnation_id: Some(incarnation_id),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );

        let error = apply_session_command(
            &mut app,
            SessionCommand::Leave {
                mutation_id: uuid::Uuid::now_v7().to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
        )
        .await
        .expect_err("owner leave should fail");

        assert!(error.to_string().contains("owner cannot leave"));
        assert!(app.state.discovery.session(session_id).is_some());
    }

    #[tokio::test]
    async fn participant_remote_lifecycle_mutations_are_rejected_explicitly() {
        let mut app = test_app();
        let session_id = SessionId::new();
        let summary = SessionSummary::new_remote(
            session_id,
            "Remote".to_owned(),
            "Owner".to_owned(),
            None,
            ShareScope::Room,
            AccessLevel::View,
            TerminalSize::default(),
        );
        let incarnation_id = uuid::Uuid::now_v7();
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary,
                incarnation_id: Some(incarnation_id),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );

        for command in [
            SessionCommand::Stop {
                request_id: uuid::Uuid::now_v7().to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
            SessionCommand::Interrupt {
                request_id: uuid::Uuid::now_v7().to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
        ] {
            let error = apply_session_command(&mut app, command)
                .await
                .expect_err("participant remote lifecycle mutation should fail");
            assert!(error.to_string().contains("not its owner"));
        }
    }

    #[tokio::test]
    async fn remote_delete_and_reopen_are_rejected_instead_of_falling_through_local() {
        let mut app = test_app();
        let session_id = SessionId::new();
        let mut summary = SessionSummary::new_remote(
            session_id,
            "Owned elsewhere".to_owned(),
            "Owner".to_owned(),
            None,
            ShareScope::Room,
            AccessLevel::Inject,
            TerminalSize::default(),
        );
        summary.role = SessionRole::Owner;
        let incarnation_id = uuid::Uuid::now_v7();
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary,
                incarnation_id: Some(incarnation_id),
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );

        for command in [
            SessionCommand::Close {
                request_id: uuid::Uuid::now_v7().to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
            SessionCommand::Delete {
                request_id: uuid::Uuid::now_v7().to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
            SessionCommand::Reopen {
                request_id: uuid::Uuid::now_v7().to_string(),
                session_id: session_id.to_string(),
                expected_runtime_incarnation_id: incarnation_id.to_string(),
            },
        ] {
            let error = apply_session_command(&mut app, command)
                .await
                .expect_err("remote local-only mutation should fail");
            assert!(error.to_string().contains("cannot"));
            assert!(error.to_string().contains("remote session"));
        }
    }

    #[tokio::test]
    async fn list_hidden_returns_correlated_runtime_owned_entries() {
        let mut app = test_app();
        let session_id = SessionId::new();
        let mut summary = SessionSummary::new_remote(
            session_id,
            "Hidden agent".to_owned(),
            "Alice".to_owned(),
            None,
            ShareScope::Room,
            AccessLevel::View,
            TerminalSize::default(),
        );
        summary.working_dir = Some("/repo/project".to_owned());
        app.state
            .discovery
            .set_hidden_session_ids(std::collections::BTreeSet::from([session_id]));
        app.state.discovery.replace_remote_sessions(
            vec![RemoteSessionRecord {
                summary,
                incarnation_id: None,
                room_id: None,
                connection_state: None,
                connection_reason: None,
                access_state: None,
                access_reason: None,
                access_issue: None,
                viewer_blocked: false,
                viewer_hidden: false,
            }],
            false,
        );

        let effects = apply_session_command(
            &mut app,
            SessionCommand::ListHidden {
                request_id: "req-hidden".to_owned(),
            },
        )
        .await
        .expect("list hidden");

        std::assert_matches!(
            effects.events.as_slice(),
            [SessionEvent::HiddenList {
                request_id,
                entries,
            }] if request_id == "req-hidden"
                && entries.len() == 1
                && entries[0].id == session_id.to_string()
                && entries[0].name == "Hidden agent"
                && entries[0].project == "/repo/project"
                && entries[0].owner == "Alice"
        );
    }
}
