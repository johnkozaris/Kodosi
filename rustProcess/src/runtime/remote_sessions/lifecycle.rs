use super::{
    AppError, Arc, BTreeSet, CancellationToken, ConnectionState, DeviceKeyAccess, DeviceKeys,
    IdentitySessionKeyTrust, OffsetDateTime, RemoteRelayMode, RemoteSessionAccessState,
    ReplayCursor, Result, Runtime, SessionId, SessionRelayClientSpec, SessionRelayEvent,
    SessionRole, SessionState, ShareScope, ShelfItem, mpsc, session_relay,
    spawn_session_relay_event_bridge,
};
struct SessionRelayLaunch {
    spec: SessionRelayClientSpec,
    relay_mode: RemoteRelayMode,
    share_scope: ShareScope,
    cancellation: CancellationToken,
}

pub(crate) async fn open(app: &mut Runtime, id: SessionId) -> Result<()> {
    if !app.remote_surfaces_ready() {
        return Err(AppError::Unsupported {
            reason: "remote operations are not ready; retry after reconciliation".to_owned(),
        });
    }

    if app.state.local.sessions.record(id).is_some() {
        app.state.shelf.activate(ShelfItem::Owned(id));
        app.state.record_log(format!(
            "{} is retained locally; opened the local session",
            id.short()
        ));
        return Ok(());
    }

    if let Some(local_id) = app
        .state
        .sharing
        .shared_sessions
        .local_id_for_backend(&id.to_string())
    {
        app.state.shelf.activate(ShelfItem::Owned(local_id));
        app.state.record_log(format!(
            "{} is hosted locally as {}; opened the local session",
            id.short(),
            local_id.short()
        ));
        return Ok(());
    }

    let record = app
        .state
        .discovery
        .session(id)
        .ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "remote session {} is not available in the current discovery catalog",
                id.short()
            ),
        })?;
    if matches!(
        record.summary.state,
        SessionState::Stopped | SessionState::Failed
    ) {
        return Err(AppError::Unsupported {
            reason: format!("remote session {} is no longer live", id.short()),
        });
    }

    if record.viewer_blocked {
        return Err(AppError::Unsupported {
            reason: record.access_reason.clone().unwrap_or_else(|| {
                format!("access to remote session {} is not available", id.short())
            }),
        });
    }

    ensure_backend_configured(app)?;

    ensure_session_relay(app, id).await?;

    app.state.shelf.activate(ShelfItem::Remote(id));
    app.state
        .record_log(format!("{} opened remote session", id.short()));
    Ok(())
}

pub(crate) async fn hide(app: &mut Runtime, id: SessionId) -> Result<bool> {
    if app
        .state
        .discovery
        .session(id)
        .is_none_or(|record| record.viewer_hidden)
    {
        return Ok(false);
    }
    let user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    app.hidden_session_store.set_hidden(&user_id, id, true)?;
    if !app.state.discovery.hide_session(id) {
        return Ok(false);
    }
    if let Some(record) = app.state.discovery.session_mut(id) {
        record.summary.last_update = OffsetDateTime::now_utc();
    }

    let visible_remote_ids = app.state.discovery.non_hidden_remote_session_ids();
    app.state.shelf.sync_remote_sessions(&visible_remote_ids);
    app.state.remove_session_relay_gracefully(id).await;
    app.reject_pending_remote_resizes(
        id,
        None,
        "the remote terminal was hidden before applying this size",
    );
    app.state
        .record_log(format!("{} hid remote session from sidebar", id.short()));
    Ok(true)
}

pub(crate) fn leave(
    app: &mut Runtime,
    id: SessionId,
    expected_runtime_incarnation_id: uuid::Uuid,
    mutation_id: uuid::Uuid,
) -> Result<()> {
    let record = app
        .state
        .discovery
        .session(id)
        .ok_or_else(|| AppError::Unsupported {
            reason: "shared session is no longer available".to_owned(),
        })?;
    if record.summary.role == SessionRole::Owner {
        return Err(AppError::Unsupported {
            reason: "the owner cannot leave their own session; stop or unshare it instead"
                .to_owned(),
        });
    }
    let incarnation_id = record.incarnation_id.ok_or_else(|| AppError::Unsupported {
        reason: "session detail is unavailable; refresh before leaving the session".to_owned(),
    })?;
    ensure_backend_configured(app)?;

    let (prepared, dispatch) = crate::runtime::sharing::prepare_leave_access_mutation(
        app,
        id,
        expected_runtime_incarnation_id,
        id.to_string(),
        incarnation_id,
        mutation_id,
    )?;
    crate::runtime::sharing::queue_access_mutation_dispatch(app, &prepared, dispatch);
    Ok(())
}

pub(crate) fn unhide(app: &mut Runtime, id: SessionId) -> Result<bool> {
    if app
        .state
        .discovery
        .session(id)
        .is_none_or(|record| !record.viewer_hidden)
    {
        return Ok(false);
    }
    let user_id = app
        .state
        .identity
        .auth
        .subject_string()
        .ok_or(AppError::Unauthorized)?;
    app.hidden_session_store.set_hidden(&user_id, id, false)?;
    if !app.state.discovery.unhide_session(id) {
        return Ok(false);
    }
    if let Some(record) = app.state.discovery.session_mut(id) {
        record.summary.last_update = OffsetDateTime::now_utc();
    }
    app.state
        .record_log(format!("{} unhid remote session", id.short()));
    Ok(true)
}

pub(crate) async fn reconcile_relays(app: &mut Runtime) -> Result<()> {
    let desired = visible_remote_session_ids(app);
    let active_ids = app.state.remote.session_relays.ids();

    for id in active_ids {
        let should_keep = desired.contains(&id)
            && app.state.discovery.session(id).is_some_and(|record| {
                app.state.remote.session_relays.matches_live(
                    id,
                    record.summary.scope,
                    remote_relay_mode_for_record(record.summary.role),
                )
            });
        if !should_keep {
            app.state.remove_session_relay_gracefully(id).await;
            app.reject_pending_remote_resizes(
                id,
                None,
                "the remote terminal connection changed before applying this size",
            );
        }
    }

    for id in desired {
        if !app.state.remote.session_relays.contains(id) {
            start_session_relay(app, id).await?;
        }
    }

    app.state.sync_session_relay_status();
    Ok(())
}

async fn ensure_session_relay(app: &mut Runtime, id: SessionId) -> Result<()> {
    let Some(record) = app.state.discovery.session(id) else {
        return start_session_relay(app, id).await;
    };

    if app.state.remote.session_relays.matches_live(
        id,
        record.summary.scope,
        remote_relay_mode_for_record(record.summary.role),
    ) {
        return Ok(());
    }

    app.state.remove_session_relay_gracefully(id).await;
    app.reject_pending_remote_resizes(
        id,
        None,
        "the remote terminal connection restarted before applying this size",
    );
    start_session_relay(app, id).await
}

fn visible_remote_session_ids(app: &Runtime) -> BTreeSet<SessionId> {
    app.state
        .shelf
        .visible_sessions()
        .iter()
        .filter_map(|item| match item {
            ShelfItem::Remote(id) => app
                .state
                .discovery
                .session(*id)
                .filter(|record| {
                    app.state.local.sessions.record(*id).is_none()
                        && !record.viewer_blocked
                        && !record.viewer_hidden
                        && !matches!(
                            record.summary.state,
                            SessionState::Stopped | SessionState::Failed
                        )
                })
                .map(|_| *id),
            ShelfItem::Owned(_) => None,
        })
        .collect()
}

async fn start_session_relay(app: &mut Runtime, id: SessionId) -> Result<()> {
    ensure_backend_configured(app)?;

    let SessionRelayLaunch {
        spec,
        relay_mode,
        share_scope,
        cancellation,
    } = prepare_session_relay_launch(app, id).await?;

    let account_origin = app
        .state
        .identity
        .current_account_event_origin()
        .ok_or(AppError::Unauthorized)?;
    let relay_generation = app.state.remote.session_relays.reserve_generation(id)?;
    let (relay_event_tx, relay_event_rx) = mpsc::channel::<SessionRelayEvent>(64);
    drop(spawn_session_relay_event_bridge(
        account_origin,
        relay_event_rx,
        app.session_events_tx.clone(),
        relay_generation,
    ));

    let handle = session_relay::spawn(
        spec,
        app.backend.clone(),
        Arc::new(IdentitySessionKeyTrust::new(app.pin_store.clone())),
        app.session_relay_ws.clone(),
        crate::runtime::auth::backend_auth_provider(app),
        relay_event_tx,
        cancellation,
    );

    app.state
        .attach_session_relay(id, handle, share_scope, relay_mode);
    app.state.record_log(format!(
        "{} relay connecting for {}",
        match relay_mode {
            RemoteRelayMode::SharedParticipant => "viewer",
            RemoteRelayMode::OwnerParticipant => "owner attachment",
        },
        id.short()
    ));
    Ok(())
}

fn ensure_backend_configured(app: &Runtime) -> Result<()> {
    if !app.backend.is_configured() {
        return Err(AppError::MissingConfig { key: "backend.api" });
    }
    Ok(())
}

async fn prepare_session_relay_launch(
    app: &mut Runtime,
    id: SessionId,
) -> Result<SessionRelayLaunch> {
    let (relay_mode, share_scope) = session_relay_mode_and_scope(app, id)?;
    mark_session_relay_connecting(app, id);
    let device_keys = load_session_relay_device_keys(app, id).await?;

    let owner_user_id = app
        .state
        .discovery
        .session(id)
        .and_then(|record| record.summary.owner_id.as_ref().map(ToString::to_string));
    let backend_incarnation_id = app
        .state
        .discovery
        .session(id)
        .and_then(|record| record.incarnation_id)
        .ok_or_else(|| AppError::Unsupported {
            reason: "session detail is unavailable; refresh before connecting".to_owned(),
        })?;

    Ok(SessionRelayLaunch {
        spec: SessionRelayClientSpec {
            id,
            backend_session_id: id.to_string(),
            backend_incarnation_id,
            cursor: ReplayCursor::default(),
            relay_mode,
            session_key: None,
            viewer_kem_secret_bytes: device_keys
                .as_ref()
                .map(|keys| keys.kem_secret_bytes_raw().to_vec()),
            viewer_device_id: device_keys.as_ref().map(|keys| keys.device_id.clone()),
            viewer_user_id: app
                .state
                .identity
                .auth
                .subject()
                .map(|user_id| user_id.to_string()),
            viewer_signing_pkcs8: device_keys
                .as_ref()
                .map(|keys| keys.signing_pkcs8_bytes().to_vec()),
            owner_user_id,
        },
        relay_mode,
        share_scope,
        cancellation: app.shutdown.child_token(),
    })
}

fn session_relay_mode_and_scope(
    app: &Runtime,
    id: SessionId,
) -> Result<(RemoteRelayMode, ShareScope)> {
    app.state
        .discovery
        .session(id)
        .map(|record| {
            (
                remote_relay_mode_for_record(record.summary.role),
                record.summary.scope,
            )
        })
        .ok_or_else(|| AppError::Unsupported {
            reason: format!("remote session {} is no longer available", id.short()),
        })
}

fn mark_session_relay_connecting(app: &mut Runtime, id: SessionId) {
    if let Some(record) = app.state.discovery.session_mut(id) {
        record.connection_state = Some(ConnectionState::Connecting);
        record.connection_reason = None;
        record.access_state = Some(RemoteSessionAccessState::RegisteringDevice);
        record.access_reason = None;
        record.access_issue = None;
        record.summary.last_update = OffsetDateTime::now_utc();
    }
}

async fn load_session_relay_device_keys(
    app: &mut Runtime,
    id: SessionId,
) -> Result<Option<DeviceKeys>> {
    let registration_result =
        crate::runtime::identity::device_keys::ensure_device_keys_registered_required(
            &app.state.identity.auth,
            &app.backend,
            &app.device_key_store,
            &app.pin_store,
        )
        .await;
    if let Err(error) = &registration_result
        && let Some(record) = app.state.discovery.session_mut(id)
    {
        record.access_state = Some(RemoteSessionAccessState::Failed);
        record.access_reason = Some(error.to_string());
        record.access_issue = None;
        record.summary.last_update = OffsetDateTime::now_utc();
    }
    registration_result?;

    Ok(Some(
        DeviceKeyAccess::new(&app.state.identity.auth, &app.device_key_store)
            .load_authenticated_device_keys()?,
    ))
}

fn remote_relay_mode_for_record(role: SessionRole) -> RemoteRelayMode {
    match role {
        SessionRole::Owner => RemoteRelayMode::OwnerParticipant,
        SessionRole::Viewer => RemoteRelayMode::SharedParticipant,
    }
}
