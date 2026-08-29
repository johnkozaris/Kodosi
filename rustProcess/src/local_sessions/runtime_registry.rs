use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use tokio_util::sync::CancellationToken;

use crate::{
    AppError, Result,
    session_runtime::handles::{
        OwnedSessionHandle, SessionPtyInstruction, SessionRuntimeHandle, SessionScreenInstruction,
        SessionSenders,
    },
};
use kodosi_domain::ids::SessionId;

#[derive(Clone, Debug, Default)]
pub(crate) struct LocalInputRouter {
    entries: Arc<RwLock<HashMap<SessionId, LocalInputTarget>>>,
}

#[derive(Clone, Debug)]
struct LocalInputTarget {
    local_incarnation_id: uuid::Uuid,
    senders: SessionSenders,
    size_authority: Arc<super::size_authority::SizeAuthorityCell>,
}

#[derive(Debug)]
pub(crate) struct OwnedSessionRuntimeRegistry {
    entries: HashMap<SessionId, OwnedSessionRuntime>,
    input_router: LocalInputRouter,

    host_theme_dark: bool,
}

impl Default for OwnedSessionRuntimeRegistry {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            input_router: LocalInputRouter::default(),
            host_theme_dark: true,
        }
    }
}

#[derive(Debug)]
struct OwnedSessionRuntime {
    senders: SessionSenders,
    cancellation: CancellationToken,
    join_handle: tokio::task::JoinHandle<()>,

    size_authority: Arc<super::size_authority::SizeAuthorityCell>,
}

impl OwnedSessionRuntimeRegistry {
    pub(crate) fn attach(
        &mut self,
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        handle: OwnedSessionHandle,
    ) {
        self.remove(id);
        let runtime = OwnedSessionRuntime::new(handle);
        self.input_router.attach(
            id,
            local_incarnation_id,
            runtime.senders.clone(),
            Arc::clone(&runtime.size_authority),
        );
        self.entries.insert(id, runtime);
    }

    pub(crate) const fn host_theme_dark(&self) -> bool {
        self.host_theme_dark
    }

    pub(crate) fn contains(&self, id: SessionId) -> bool {
        self.entries.contains_key(&id)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn ids(&self) -> Vec<SessionId> {
        self.entries.keys().copied().collect()
    }

    pub(crate) fn cancel_all(&self) {
        for runtime in self.entries.values() {
            runtime.cancellation.cancel();
        }
    }

    pub(crate) fn release_remote_size_authority(&self, id: SessionId) {
        if let Some(runtime) = self.entries.get(&id) {
            runtime.size_authority.release_remote();
        }
    }

    pub(crate) fn size_authority(
        &self,
        id: SessionId,
    ) -> Option<Arc<super::size_authority::SizeAuthorityCell>> {
        self.entries
            .get(&id)
            .map(|runtime| Arc::clone(&runtime.size_authority))
    }

    pub(crate) fn remove(&mut self, id: SessionId) -> bool {
        let removed = self.entries.remove(&id).is_some();
        if removed {
            self.input_router.remove(id);
        }
        removed
    }

    pub(crate) fn input_router(&self) -> LocalInputRouter {
        self.input_router.clone()
    }

    pub(crate) fn runtime_handle(&self, id: SessionId) -> Option<SessionRuntimeHandle> {
        let runtime = self.entries.get(&id)?;
        Some(SessionRuntimeHandle::new(id, runtime.senders.clone()))
    }

    pub(crate) fn child_cancellation_token(&self, id: SessionId) -> Result<CancellationToken> {
        self.entries
            .get(&id)
            .map(|runtime| runtime.cancellation.child_token())
            .ok_or(AppError::NoActiveSession)
    }

    pub(crate) async fn send_to_screen(
        &self,
        id: SessionId,
        instruction: SessionScreenInstruction,
    ) -> Result<()> {
        let runtime = self.runtime_handle(id).ok_or(AppError::NoActiveSession)?;
        runtime
            .send_screen_instruction(instruction)
            .await
            .inspect_err(|error| {
                if let AppError::ChannelFull { .. } = error {
                    tracing::warn!(
                        session_id = %id,
                        "coordinator command queue stayed full past timeout"
                    );
                } else {
                    tracing::warn!(
                        session_id = %id,
                        "coordinator command channel unavailable"
                    );
                }
            })
    }

    pub(crate) async fn send_to_pty(
        &self,
        id: SessionId,
        instruction: SessionPtyInstruction,
    ) -> Result<()> {
        let runtime = self.runtime_handle(id).ok_or(AppError::NoActiveSession)?;
        runtime
            .send_pty_instruction(instruction)
            .await
            .inspect_err(|error| {
                if let AppError::ChannelFull { .. } = error {
                    tracing::warn!(
                        session_id = %id,
                        "coordinator PTY queue stayed full past timeout"
                    );
                } else {
                    tracing::warn!(
                        session_id = %id,
                        "coordinator PTY channel unavailable"
                    );
                }
            })
    }

    pub(crate) async fn sync_clipboard_support(
        &self,
        ids: &[SessionId],
        supported: bool,
    ) -> Vec<String> {
        let mut messages = Vec::new();
        for id in ids {
            let Some(runtime) = self.runtime_handle(*id) else {
                continue;
            };
            if let Err(error) = runtime.set_clipboard_support(supported).await {
                messages.push(format!(
                    "{} failed to update terminal clipboard support: {error}",
                    id.short()
                ));
            }
        }
        messages
    }

    pub(crate) fn try_notify_theme_changed(
        &mut self,
        ids: &[SessionId],
        dark: bool,
    ) -> Vec<String> {
        self.host_theme_dark = dark;
        let mut messages = Vec::new();
        for id in ids {
            let Some(runtime) = self.entries.get(id) else {
                continue;
            };
            if let Err(error) = runtime
                .senders
                .try_send_to_screen(*id, SessionScreenInstruction::ThemeChanged { dark })
            {
                messages.push(format!(
                    "{} failed to notify theme change: {error}",
                    id.short()
                ));
            }
        }
        messages
    }

    pub(crate) fn try_sync_clipboard_support(
        &self,
        ids: &[SessionId],
        supported: bool,
    ) -> Vec<String> {
        let mut messages = Vec::new();
        for id in ids {
            let Some(runtime) = self.entries.get(id) else {
                continue;
            };
            if let Err(error) = runtime.senders.try_send_to_screen(
                *id,
                SessionScreenInstruction::SetClipboardSupport { supported },
            ) {
                messages.push(format!(
                    "{} failed to update terminal clipboard support: {error}",
                    id.short()
                ));
            }
        }
        messages
    }

    pub(crate) fn finished_ids(&self) -> Vec<SessionId> {
        self.entries
            .iter()
            .filter_map(|(id, runtime)| runtime.join_handle.is_finished().then_some(*id))
            .collect()
    }
}

impl LocalInputRouter {
    fn attach(
        &self,
        id: SessionId,
        local_incarnation_id: uuid::Uuid,
        senders: SessionSenders,
        size_authority: Arc<super::size_authority::SizeAuthorityCell>,
    ) {
        self.write_entries().insert(
            id,
            LocalInputTarget {
                local_incarnation_id,
                senders,
                size_authority,
            },
        );
    }

    fn remove(&self, id: SessionId) {
        self.write_entries().remove(&id);
    }

    pub(crate) async fn send_input(
        &self,
        id: SessionId,
        expected_runtime_incarnation_id: Option<uuid::Uuid>,
        input: crate::session_runtime::commands::SessionInput,
    ) -> Option<Result<()>> {
        let target = self.read_entries().get(&id).cloned()?;
        if expected_runtime_incarnation_id
            .is_some_and(|expected| expected != target.local_incarnation_id)
        {
            return Some(Err(AppError::Unsupported {
                reason: "terminal input targets a stale runtime incarnation".to_owned(),
            }));
        }
        let runtime = SessionRuntimeHandle::new(id, target.senders);
        let mut claim = if super::size_authority::SizeAuthorityCell::input_claims_authority(&input)
        {
            let claim = target
                .size_authority
                .begin_claim(super::size_authority::SizeOrigin::Local)
                .await;
            let reclaim_size = match claim.preview() {
                super::size_authority::ClaimPreview::Reclaim(size) => size,
                super::size_authority::ClaimPreview::Unavailable
                | super::size_authority::ClaimPreview::AlreadyHeld => None,
            };
            if let Some(size) = reclaim_size {
                let result = runtime
                    .send_screen_instruction(SessionScreenInstruction::ResizeAndInputBatch {
                        size,
                        inputs: vec![input],
                    })
                    .await;
                if result.is_ok() {
                    claim.commit();
                }
                return Some(result);
            }
            Some(claim)
        } else {
            None
        };
        let result = runtime
            .send_screen_instruction(SessionScreenInstruction::Input(input))
            .await;
        if result.is_ok()
            && let Some(claim) = claim.take()
        {
            claim.commit();
        }
        drop(claim);
        Some(result)
    }

    fn read_entries(&self) -> std::sync::RwLockReadGuard<'_, HashMap<SessionId, LocalInputTarget>> {
        self.entries.read().unwrap_or_else(|poisoned| {
            tracing::warn!("local input router read lock was poisoned; recovering");
            poisoned.into_inner()
        })
    }

    fn write_entries(
        &self,
    ) -> std::sync::RwLockWriteGuard<'_, HashMap<SessionId, LocalInputTarget>> {
        self.entries.write().unwrap_or_else(|poisoned| {
            tracing::warn!("local input router write lock was poisoned; recovering");
            poisoned.into_inner()
        })
    }
}

impl OwnedSessionRuntime {
    fn new(handle: OwnedSessionHandle) -> Self {
        Self {
            senders: handle.senders,
            cancellation: handle.cancellation,
            join_handle: handle.join_handle,
            size_authority: Arc::default(),
        }
    }
}

impl Drop for OwnedSessionRuntime {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if !self.join_handle.is_finished() {
            self.join_handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OwnedSessionRuntimeRegistry;
    use crate::{
        local_sessions::size_authority::SizeOrigin,
        session_runtime::{
            commands::SessionInput,
            handles::{
                OwnedSessionHandle, SessionPtyInstruction, SessionScreenInstruction, SessionSenders,
            },
        },
    };
    use kodosi_domain::{ids::SessionId, terminal::TerminalSize};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn pending_handle() -> (
        OwnedSessionHandle,
        CancellationToken,
        tokio::task::AbortHandle,
    ) {
        let (screen_tx, _screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let cancellation = CancellationToken::new();
        let join_handle = tokio::spawn(std::future::pending::<()>());
        let abort_handle = join_handle.abort_handle();
        (
            OwnedSessionHandle {
                senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
                cancellation: cancellation.clone(),
                join_handle,
            },
            cancellation,
            abort_handle,
        )
    }

    async fn wait_for_finished(abort_handle: &tokio::task::AbortHandle) {
        for _ in 0..10 {
            if abort_handle.is_finished() {
                return;
            }
            tokio::task::yield_now().await;
        }
        assert!(abort_handle.is_finished());
    }

    fn incarnation() -> uuid::Uuid {
        uuid::Uuid::now_v7()
    }

    #[tokio::test]
    async fn remove_cancels_and_aborts_runtime() {
        let id = SessionId::new();
        let (handle, cancellation, abort_handle) = pending_handle();
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);

        assert!(registry.remove(id));

        assert!(cancellation.is_cancelled());
        wait_for_finished(&abort_handle).await;
        assert!(!registry.contains(id));
    }

    #[tokio::test]
    async fn attach_replaces_existing_runtime_before_insert() {
        let id = SessionId::new();
        let (old_handle, old_cancellation, old_abort) = pending_handle();
        let (new_handle, new_cancellation, new_abort) = pending_handle();
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), old_handle);

        registry.attach(id, incarnation(), new_handle);

        assert!(old_cancellation.is_cancelled());
        wait_for_finished(&old_abort).await;
        assert!(!new_cancellation.is_cancelled());
        assert!(registry.contains(id));

        registry.remove(id);
        new_abort.abort();
    }

    #[tokio::test]
    async fn try_sync_clipboard_support_dispatches_terminal_update() {
        let id = SessionId::new();
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);

        let messages = registry.try_sync_clipboard_support(&[id], false);

        assert!(messages.is_empty());
        let Some(SessionScreenInstruction::SetClipboardSupport { supported }) =
            screen_rx.recv().await
        else {
            panic!("expected clipboard support update");
        };
        assert!(!supported);
    }

    #[tokio::test]
    async fn send_to_screen_reports_missing_runtime() {
        let registry = OwnedSessionRuntimeRegistry::default();
        let id = SessionId::new();
        let size = TerminalSize::new(8, 20).unwrap_or_else(|error| panic!("fixed size: {error}"));

        let error = registry
            .send_to_screen(
                id,
                SessionScreenInstruction::Resize {
                    size,
                    pixel_geometry: None,
                    completion: None,
                },
            )
            .await
            .expect_err("missing runtime should reject screen sends");

        std::assert_matches!(error, crate::AppError::NoActiveSession);
    }

    #[tokio::test]
    async fn cloned_input_router_dispatches_without_borrowing_runtime_state() {
        let id = SessionId::new();
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(4);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);
        let router = registry.input_router();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            router.send_input(id, None, SessionInput::new(b"x".to_vec())),
        )
        .await
        .expect("local input router must not wait on the runtime state loop");

        std::assert_matches!(result, Some(Ok(())));
        let Some(SessionScreenInstruction::Input(input)) = screen_rx.recv().await else {
            panic!("expected screen input");
        };
        assert_eq!(input.as_bytes(), b"x");
    }

    #[tokio::test]
    async fn cloned_input_router_reclaims_with_one_ordered_instruction() {
        let id = SessionId::new();
        let local_size = TerminalSize::new(30, 100).expect("local size");
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);
        let authority = registry.size_authority(id).expect("size authority");
        assert!(authority.admit_resize(SizeOrigin::Local, local_size));
        authority.begin_claim(SizeOrigin::Remote).await.commit();

        let result = registry
            .input_router()
            .send_input(id, None, SessionInput::new(b"local".to_vec()))
            .await;

        std::assert_matches!(result, Some(Ok(())));
        let Some(SessionScreenInstruction::ResizeAndInputBatch { size, inputs }) =
            screen_rx.recv().await
        else {
            panic!("expected one atomic reclaim instruction");
        };
        assert_eq!(size, local_size);
        assert_eq!(inputs, vec![SessionInput::new(b"local".to_vec())]);
        assert!(screen_rx.try_recv().is_err());
        assert!(!authority.admit_resize(
            SizeOrigin::Remote,
            TerminalSize::new(40, 120).expect("remote size")
        ));
    }

    #[tokio::test]
    async fn closed_screen_lane_does_not_commit_input_claim() {
        let id = SessionId::new();
        let (screen_tx, screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        drop(screen_rx);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);
        let authority = registry.size_authority(id).expect("size authority");
        assert!(authority.admit_resize(
            SizeOrigin::Local,
            TerminalSize::new(30, 100).expect("local size")
        ));
        authority.begin_claim(SizeOrigin::Remote).await.commit();

        let result = registry
            .input_router()
            .send_input(id, None, SessionInput::new(b"local".to_vec()))
            .await;

        std::assert_matches!(result, Some(Err(crate::AppError::ChannelClosed { .. })));
        assert!(!authority.admit_resize(
            SizeOrigin::Local,
            TerminalSize::new(31, 101).expect("new local size")
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn full_screen_lane_does_not_commit_input_claim() {
        let id = SessionId::new();
        let (screen_tx, _screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let senders = SessionSenders::new(Some(screen_tx), Some(pty_tx));
        senders
            .try_send_to_screen(
                id,
                SessionScreenInstruction::Focus {
                    client_id: "occupied".to_owned(),
                },
            )
            .expect("fill screen lane");
        let handle = OwnedSessionHandle {
            senders,
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);
        let authority = registry.size_authority(id).expect("size authority");
        assert!(authority.admit_resize(
            SizeOrigin::Local,
            TerminalSize::new(30, 100).expect("local size")
        ));
        authority.begin_claim(SizeOrigin::Remote).await.commit();

        let result = registry
            .input_router()
            .send_input(id, None, SessionInput::new(b"local".to_vec()))
            .await;

        std::assert_matches!(result, Some(Err(crate::AppError::ChannelFull { .. })));
        assert!(!authority.admit_resize(
            SizeOrigin::Local,
            TerminalSize::new(31, 101).expect("new local size")
        ));
    }

    #[tokio::test]
    async fn stale_incarnation_input_never_reaches_replacement_runtime() {
        let id = SessionId::new();
        let old_incarnation = incarnation();
        let new_incarnation = incarnation();
        let (old_handle, _, old_abort) = pending_handle();
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(4);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let new_handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, old_incarnation, old_handle);
        let router = registry.input_router();
        registry.attach(id, new_incarnation, new_handle);
        wait_for_finished(&old_abort).await;

        let stale = router
            .send_input(
                id,
                Some(old_incarnation),
                SessionInput::new(b"stale".to_vec()),
            )
            .await;
        std::assert_matches!(stale, Some(Err(crate::AppError::Unsupported { .. })));
        assert!(screen_rx.try_recv().is_err());

        let current = router
            .send_input(
                id,
                Some(new_incarnation),
                SessionInput::new(b"current".to_vec()),
            )
            .await;
        std::assert_matches!(current, Some(Ok(())));
        let Some(SessionScreenInstruction::Input(input)) = screen_rx.recv().await else {
            panic!("expected current screen input");
        };
        assert_eq!(input.as_bytes(), b"current");
    }

    #[tokio::test]
    async fn finished_runtime_is_observable_without_exposing_join_handle() {
        let id = SessionId::new();
        let (screen_tx, _screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(async {}),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);
        tokio::task::yield_now().await;

        assert_eq!(registry.finished_ids(), vec![id]);
    }

    #[tokio::test]
    async fn runtime_handle_encapsulates_senders() {
        let id = SessionId::new();
        let (screen_tx, mut screen_rx) = mpsc::channel::<SessionScreenInstruction>(1);
        let (pty_tx, _pty_rx) = mpsc::channel::<SessionPtyInstruction>(1);
        let handle = OwnedSessionHandle {
            senders: SessionSenders::new(Some(screen_tx), Some(pty_tx)),
            cancellation: CancellationToken::new(),
            join_handle: tokio::spawn(std::future::pending::<()>()),
        };
        let mut registry = OwnedSessionRuntimeRegistry::default();
        registry.attach(id, incarnation(), handle);

        registry
            .runtime_handle(id)
            .expect("runtime handle should exist")
            .send_screen_instruction(SessionScreenInstruction::Input(SessionInput::new(
                b"hello".to_vec(),
            )))
            .await
            .expect("screen send should succeed");

        let Some(SessionScreenInstruction::Input(input)) = screen_rx.recv().await else {
            panic!("expected screen input");
        };
        assert_eq!(input.as_bytes(), b"hello");
    }
}
