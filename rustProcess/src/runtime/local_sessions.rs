use time::OffsetDateTime;

use crate::{
    AppError, Result,
    local_sessions::{lifecycle::LocalSessionLifecycleCtx, ops::reject_non_kodosi_local},
};
use kodosi_domain::{ids::SessionId, session::SessionState};

use super::Runtime;

pub(crate) fn lifecycle(app: &mut Runtime) -> LocalSessionLifecycleCtx<'_> {
    let allow_terminal_clipboard_write =
        app.state.config.permissions.allow_terminal_clipboard_write;
    LocalSessionLifecycleCtx {
        state: &mut app.state,
        catalog: &app.local_catalog,
        session_events_tx: &app.session_events_tx,
        shutdown: &app.shutdown,
        clipboard_available: app.clipboard.is_available(),
        allow_terminal_clipboard_write,
        terminal_hub: &app.terminal_hub,
    }
}

pub(crate) async fn rename(app: &mut Runtime, id: SessionId, title: String) -> Result<()> {
    let should_patch_backend = {
        let record = app
            .state
            .local
            .sessions
            .record(id)
            .ok_or(AppError::NoActiveSession)?;

        reject_non_kodosi_local(
            id,
            record.summary.provenance,
            "rename it from the owner runtime instead",
        )?;

        if record.summary.title == title {
            return Ok(());
        }

        app.state.sharing.shared_sessions.contains(id)
            && !matches!(
                record.summary.state,
                SessionState::Stopped | SessionState::Failed
            )
    };

    if should_patch_backend {
        crate::runtime::sharing::update_backend_session_title(app, id, &title).await?;
    }

    if let Some(record) = app.state.local.sessions.record_mut(id) {
        record.summary.title.clone_from(&title);
        record.summary.last_update = OffsetDateTime::now_utc();
    }

    if let Some(record) = app.state.local.sessions.record(id) {
        app.local_catalog.persist_record(record)?;
    }

    app.state
        .shelf
        .sync_owned_sessions(app.state.local.sessions.ids());
    app.state
        .record_log(format!("{} renamed session to {}", id.short(), title));
    Ok(())
}
