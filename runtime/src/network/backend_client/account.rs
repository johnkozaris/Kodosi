use super::*;
use crate::protocol::AuthRestoringReason;

const ENROLLMENT_CHECK_MS: u64 = 5 * 60 * 1000;
const DELETION_POLL: Duration = Duration::from_secs(3);
pub(crate) const DELETION_LIFE: Duration = Duration::from_mins(15);

enum Deletion {
    Waiting,
    Done,
    GaveUp,
}

impl BackendClient {
    pub(super) async fn restore_saved(&self, generation: u64) -> Result<()> {
        terminal_connections::check_generation(self, generation)?;
        if self.identity().is_some() || !self.inner.restore_pending.load(Ordering::Acquire) {
            return Ok(());
        }
        let secrets = self.inner.state.lock().await.secrets.clone();
        let cancel = self
            .inner
            .login_interrupt
            .lock()
            .map_err(|_| Error::Closed)?
            .clone();
        let saved = match secrets.run(cancel, |store| store.load("tokens")).await {
            Ok(saved) => saved,
            Err(error) => {
                if !matches!(error, Error::Busy | Error::Stale | Error::Closed) {
                    self.announce_restoring(generation, AuthRestoringReason::Unreadable);
                }
                return Err(error);
            }
        };
        if let Some(saved) = saved {
            let restored = async {
                let mut tokens: Tokens = serde_json::from_str(&saved)?;
                if tokens.expires_at <= identity::now_ms() + 60_000 {
                    tokens = self.oidc()?.refresh(&tokens).await?;
                }
                self.finish_login(tokens, generation).await
            }
            .await;
            if let Err(error) = &restored {
                self.restoring(generation, error);
            }
            restored?;
            self.stop_restoring();
        } else {
            self.stop_restoring();
            self.emit_for(
                generation,
                None,
                json!({"type":"auth.required","reason":"signedOut"}),
            );
        }
        self.inner.restore_pending.store(false, Ordering::Release);
        Ok(())
    }

    fn restoring(&self, generation: u64, error: &Error) {
        let reason = match error {
            Error::UpdateRequired => AuthRestoringReason::UpdateRequired,
            Error::Dns | Error::Unreachable | Error::Timeout => AuthRestoringReason::Offline,
            Error::SignedOut | Error::AccountDeleted | Error::Stale | Error::Closed => return,
            _ => AuthRestoringReason::Unavailable,
        };
        self.announce_restoring(generation, reason);
    }

    fn announce_restoring(&self, generation: u64, reason: AuthRestoringReason) {
        if reason == AuthRestoringReason::UpdateRequired {
            self.update_required();
        }
        let Ok(mut restoring) = self.inner.restoring.lock() else {
            return;
        };
        if restoring.replace(reason) != Some(reason) {
            drop(restoring);
            let mut event = json!({"type":"auth.restoring","reason":reason});
            self.credential_fields(&mut event);
            self.emit_for(generation, None, event);
        }
    }

    fn stop_restoring(&self) {
        if let Ok(mut restoring) = self.inner.restoring.lock() {
            *restoring = None;
        }
    }

    pub(super) fn oidc(&self) -> Result<Oidc> {
        Oidc::new(
            &self.inner.config.issuer,
            self.inner.config.client_id.clone(),
            &self.inner.config.scopes,
        )
    }

    pub(super) async fn finish_login(
        &self,
        tokens: Tokens,
        expected_generation: u64,
    ) -> Result<()> {
        let profile: Value = self
            .inner
            .http
            .bearer(Method::GET, "api/me", &tokens.access_token, None)
            .await?;
        let user = wire::text(&profile, "id")?.to_owned();
        if self.generation() != expected_generation || self.inner.shutdown.is_cancelled() {
            return Err(Error::Stale);
        }
        let secrets = self.inner.state.lock().await.secrets.clone();
        let cancel = self
            .inner
            .login_interrupt
            .lock()
            .map_err(|_| Error::Closed)?
            .clone();
        let key_user = user.clone();
        let keys = Arc::new(
            secrets
                .run(cancel.clone(), move |store| {
                    DeviceKeys::load_or_create(store, &key_user)
                })
                .await?,
        );
        let saved = Zeroizing::new(serde_json::to_string(&tokens)?);
        secrets
            .run(cancel.clone(), move |store| store.store("tokens", &saved))
            .await?;
        if cancel.is_cancelled() || self.generation() != expected_generation {
            return Err(Error::Stale);
        }
        let mut state = self.inner.state.lock().await;
        state.tokens = Some(tokens.clone());
        state.unsaved_tokens = false;
        let cancel = state.account_cancel.clone();
        drop(state);
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(Credentials {
            user_id: user.clone(),
            display_name: profile
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or("You")
                .to_owned(),
            token: Zeroizing::new(tokens.access_token.clone()),
            keys,
            enrolled: false,
            notice: None,
            generation: expected_generation,
            cancel,
        });
        self.ensure_enrolled().await?;
        if self.generation() != expected_generation {
            return Err(Error::Stale);
        }
        let enrolled = self.identity().is_some_and(|identity| identity.enrolled);
        self.emit_for(
            expected_generation,
            Some(user.clone()),
            self.ready_event(&user, enrolled),
        );
        for event in self.device_events().await? {
            self.emit_for(expected_generation, Some(user.clone()), event);
        }
        if self.identity().is_some_and(|identity| identity.enrolled) {
            self.emit_for(expected_generation, Some(user), self.session_event().await?);
        }
        Ok(())
    }

    pub(super) async fn renew_token(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let previous = { self.inner.state.lock().await.tokens.clone() };
        let Some(previous) = previous else {
            return Ok(());
        };
        if previous.expires_at > identity::now_ms() + 60_000 {
            return self.save_tokens(&credentials).await;
        }
        let tokens = self.oidc()?.refresh(&previous).await?;
        self.check_credentials(&credentials)?;
        {
            let mut state = self.inner.state.lock().await;
            state.tokens = Some(tokens.clone());
            state.unsaved_tokens = true;
        }
        if let Some(credentials) = self
            .inner
            .credentials
            .write()
            .map_err(|_| Error::Closed)?
            .as_mut()
        {
            credentials.token = Zeroizing::new(tokens.access_token.clone());
        }
        self.inner.renew_connections.store(true, Ordering::Release);
        self.save_tokens(&credentials).await
    }

    async fn save_tokens(&self, credentials: &Credentials) -> Result<()> {
        let (secrets, saved, expires_at) = {
            let state = self.inner.state.lock().await;
            match state.tokens.as_ref() {
                Some(tokens) if state.unsaved_tokens => (
                    state.secrets.clone(),
                    Zeroizing::new(serde_json::to_string(tokens)?),
                    tokens.expires_at,
                ),
                _ => return Ok(()),
            }
        };
        let result = secrets
            .run(credentials.cancel.clone(), move |store| {
                store.store("tokens", &saved)
            })
            .await;
        match result {
            Ok(()) => {
                let mut state = self.inner.state.lock().await;
                if state
                    .tokens
                    .as_ref()
                    .is_some_and(|tokens| tokens.expires_at == expires_at)
                {
                    state.unsaved_tokens = false;
                }
                state.token_save_failing = false;
                drop(state);
                Ok(())
            }
            Err(error @ (Error::Stale | Error::Closed)) => Err(error),
            Err(error) => {
                let first =
                    !std::mem::replace(&mut self.inner.state.lock().await.token_save_failing, true);
                if first {
                    tracing::warn!(%error, "the renewed sign-in was not saved; it is used and saved later");
                }
                Ok(())
            }
        }
    }

    pub(super) async fn refresh(&self) -> Result<()> {
        if !self.inner.identity_settled.load(Ordering::Acquire) {
            self.inner.restore_pending.store(true, Ordering::Release);
            return self.restore_saved(self.generation()).await;
        }
        let credentials = self.credentials()?;
        if self.inner.state.lock().await.tokens.is_none() {
            return Ok(());
        }
        self.renew_token().await?;
        if self.inner.renew_connections.load(Ordering::Acquire) {
            let renewed = self.credentials()?;
            if renewed.enrolled {
                self.inner
                    .http
                    .device::<Value>(Method::POST, "api/me/connections/renew", &renewed, None)
                    .await?;
            }
            self.inner.renew_connections.store(false, Ordering::Release);
        }
        let now = identity::now_ms();
        if now.saturating_sub(self.inner.enrollment_checked.load(Ordering::Acquire))
            >= ENROLLMENT_CHECK_MS
        {
            if let Err(error) = self.ensure_enrolled().await {
                if !error.unanswered() {
                    self.suspend_transports().await;
                }
                return Err(error);
            }
            self.inner.enrollment_checked.store(now, Ordering::Release);
            if self.credentials()?.enrolled != credentials.enrolled {
                for event in self.enrollment_events().await? {
                    self.emit_for(
                        credentials.generation,
                        Some(credentials.user_id.clone()),
                        event,
                    );
                }
            }
        }
        self.check_credentials(&credentials)?;
        self.reconcile_device_removals().await?;
        #[cfg(feature = "host")]
        self.reconcile_shares().await?;
        Ok(())
    }

    pub(super) async fn suspend_transports(&self) {
        #[cfg(feature = "host")]
        for publication in self.inner.publications.lock().await.values() {
            publication.invalidate();
        }
        for cancel in self.inner.connections.lock().await.drain(..) {
            cancel.cancel();
        }
    }

    #[cfg(feature = "host")]
    async fn end_publications(&self) -> BTreeMap<Uuid, Arc<terminal_connections::Publication>> {
        let publications = std::mem::take(&mut *self.inner.publications.lock().await);
        for publication in publications.values() {
            publication.cancel.cancel();
            publication.invalidate();
        }
        publications
    }

    pub(super) async fn cancel_transports(&self) {
        let notification = self.inner.notifications.lock().await.take();
        if let Some(cancel) = notification {
            cancel.cancel();
        }
        for cancel in self.inner.connections.lock().await.drain(..) {
            cancel.cancel();
        }
    }

    pub(super) async fn retire_account(&self, erase_tokens: bool) -> Result<()> {
        #[cfg(feature = "host")]
        let cleanup = self
            .inner
            .credentials
            .read()
            .map_err(|_| Error::Closed)?
            .clone();
        self.inner.identity_settled.store(false, Ordering::Release);
        self.inner.restore_pending.store(false, Ordering::Release);
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        let mut state = self.inner.state.lock().await;
        state.account_cancel.cancel();
        state.account_cancel = CancellationToken::new();
        self.inner
            .login_interrupt
            .lock()
            .map_err(|_| Error::Closed)?
            .cancel();
        self.stop_restoring();
        state.tokens = None;
        state.unsaved_tokens = false;
        state.token_save_failing = false;
        state.link = None;
        state.deletion = None;
        state.changed_friends.clear();
        let secrets = state.secrets.clone();
        drop(state);
        if let Ok(mut kept) = self.inner.sessions.lock() {
            kept.clear();
        }
        let result = if erase_tokens {
            let deletion = secrets.run(CancellationToken::new(), |store| store.delete("tokens"));
            tokio::time::timeout(Duration::from_secs(3), deletion)
                .await
                .unwrap_or_else(|_| {
                    Err(invalid(
                        "Credential deletion is still waiting for the system store.",
                    ))
                })
        } else {
            Ok(())
        };
        #[cfg(feature = "host")]
        let publications = self.end_publications().await;
        self.cancel_transports().await;
        #[cfg(feature = "host")]
        if let Some(mut credentials) = cleanup {
            credentials.cancel = CancellationToken::new();
            self.delete_publications(&credentials, publications).await;
        }
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = None;
        result
    }

    #[cfg(feature = "host")]
    pub(super) async fn delete_publications(
        &self,
        credentials: &Credentials,
        publications: BTreeMap<Uuid, Arc<terminal_connections::Publication>>,
    ) {
        use futures_util::{StreamExt as _, stream};
        let pending = stream::iter(publications.into_values())
            .map(|publication| async move {
                let info = publication.info.read().await.clone();
                self.inner
                    .http
                    .device::<Value>(
                        Method::DELETE,
                        &format!(
                            "api/sessions/{}?incarnationId={}",
                            info.session_id, info.incarnation_id
                        ),
                        credentials,
                        None,
                    )
                    .await
            })
            .buffer_unordered(8)
            .for_each(|result| async move {
                if let Err(error) = result {
                    tracing::warn!(%error, "remote publication cleanup was not confirmed");
                }
            });
        if tokio::time::timeout(Duration::from_secs(6), pending)
            .await
            .is_err()
        {
            tracing::warn!("remote publication cleanup timed out");
        }
    }

    pub(super) async fn logout(&self) -> Result<()> {
        self.retire_account(true).await
    }

    pub(super) async fn forget_deleted_account(&self) -> Result<()> {
        let user = self
            .inner
            .credentials
            .read()
            .map_err(|_| Error::Closed)?
            .as_ref()
            .map(|credentials| credentials.user_id.clone());
        let secrets = self.inner.state.lock().await.secrets.clone();
        self.retire_account(true).await?;
        if let Some(user) = user {
            secrets
                .run(CancellationToken::new(), move |store| {
                    DeviceKeys::forget(store, &user)
                })
                .await?;
        }
        Ok(())
    }

    pub(super) fn deletion_page(&self, page: &str) -> Result<String> {
        let issuer = self.inner.config.issuer.trim_end_matches('/');
        if page.len() > 4096
            || page.chars().any(char::is_control)
            || !page.starts_with(&format!("{issuer}/protocol/openid-connect/auth?"))
        {
            return Err(invalid(
                "The server named an account deletion page outside the sign-in service.",
            ));
        }
        Ok(page.to_owned())
    }

    pub(super) async fn watch_deletion(&self, credentials: Credentials, request: String) {
        let started = tokio::time::Instant::now();
        self.inner.state.lock().await.deletion = Some(started);
        let this = self.clone();
        tokio::spawn(async move {
            let outcome = loop {
                tokio::select! {
                    () = this.inner.shutdown.cancelled() => return,
                    () = credentials.cancel.cancelled() => break Err("The account changed before the deletion was confirmed."),
                    () = tokio::time::sleep(DELETION_POLL) => {}
                }
                let _operation = this.inner.operations.lock().await;
                if this.inner.state.lock().await.deletion != Some(started) {
                    break Err("The account deletion was cancelled.");
                }
                match this.read_deletion(started).await {
                    Ok(Deletion::Waiting) => {}
                    Ok(Deletion::Done) => break Ok(()),
                    Ok(Deletion::GaveUp) => {
                        break Err("Kodosi stopped waiting for the confirmation.");
                    }
                    Err(error) if error.user_facing() => {
                        tracing::warn!(%error, "the account deletion was not read; it will be tried again");
                    }
                    Err(_) => break Err("The account deletion could not be confirmed."),
                }
            };
            let answer = match outcome {
                Ok(()) => {
                    json!({"type":"auth.result","requestId":request,"operation":"auth.deleteAccount"})
                }
                Err(message) => {
                    json!({"type":"auth.error","requestId":request,"operation":"auth.deleteAccount","message":message})
                }
            };
            this.emit_for(
                this.generation(),
                this.identity().map(|identity| identity.user_id),
                answer,
            );
        });
    }

    pub(super) async fn deletion_confirmed(&self) -> bool {
        if self.inner.state.lock().await.deletion.is_none() {
            return false;
        }
        let Ok(credentials) = self.credentials() else {
            return false;
        };
        matches!(
            self.inner
                .http
                .bearer::<Value>(Method::GET, "api/me", &credentials.token, None)
                .await,
            Err(Error::AccountDeleted)
        )
    }

    async fn read_deletion(&self, started: tokio::time::Instant) -> Result<Deletion> {
        if started.elapsed() >= DELETION_LIFE {
            self.inner.state.lock().await.deletion = None;
            return Ok(Deletion::GaveUp);
        }
        let current = self.credentials()?;
        match self
            .inner
            .http
            .bearer::<Value>(Method::GET, "api/me", &current.token, None)
            .await
        {
            Ok(_) => Ok(Deletion::Waiting),
            Err(Error::AccountDeleted) => {
                self.sign_in_kept(Err(Error::AccountDeleted)).await?;
                Ok(Deletion::Done)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        if let Ok(credentials) = self.inner.credentials.read()
            && let Some(credentials) = credentials.as_ref()
        {
            credentials.cancel.cancel();
        }
        if let Ok(cancel) = self.inner.login_interrupt.lock() {
            cancel.cancel();
        }
        let _operation = self.inner.operations.lock().await;
        let _outcome = self.retire_account(false).await;
    }
}
