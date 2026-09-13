use super::*;

impl Network {
    pub(super) async fn restore_saved(&self, generation: u64) -> Result<()> {
        relay::check_generation(self, generation)?;
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
        let saved = secrets.run(cancel, |store| store.load("tokens")).await?;
        if let Some(saved) = saved {
            let mut tokens: Tokens = serde_json::from_str(&saved)?;
            if tokens.expires_at <= identity::now_ms() + 60_000 {
                tokens = self.oidc()?.refresh(&tokens).await?;
            }
            self.finish_login(tokens, generation).await?;
        } else {
            self.emit_for(
                generation,
                None,
                json!({"type":"auth.required","reason":"signedOut"}),
            );
        }
        self.inner.restore_pending.store(false, Ordering::Release);
        Ok(())
    }

    pub(super) fn oidc(&self) -> Result<Oidc> {
        Oidc::new(
            &self.inner.config.issuer,
            self.inner.config.client_id.clone(),
            &self.inner.config.scopes,
            self.inner.config.audience.clone(),
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
        let cancel = state.account_cancel.clone();
        drop(state);
        let credentials = Credentials {
            user_id: user.clone(),
            token: Zeroizing::new(tokens.access_token.clone()),
            keys,
            enrolled: false,
            generation: expected_generation,
            cancel,
        };
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(credentials.clone());
        *self.inner.identity.write().map_err(|_| Error::Closed)? = Some(Identity {
            user_id: user.clone(),
            device_id: credentials.keys.device_id.clone(),
            display_name: profile
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or("You")
                .to_owned(),
            enrolled: false,
        });
        self.ensure_enrolled().await?;
        self.inner.identity_settled.store(true, Ordering::Release);
        if self.generation() != expected_generation {
            return Err(Error::Stale);
        }
        self.emit_for(
            expected_generation,
            Some(user.clone()),
            json!({"type":"auth.ready","userId":user}),
        );
        for event in self.device_events().await? {
            self.emit_for(expected_generation, Some(user.clone()), event);
        }
        if self.identity().is_some_and(|identity| identity.enrolled) {
            self.emit_for(expected_generation, Some(user), self.session_event().await?);
        }
        Ok(())
    }

    pub(super) async fn refresh(&self) -> Result<()> {
        if !self.inner.identity_settled.load(Ordering::Acquire) {
            self.inner.restore_pending.store(true, Ordering::Release);
            return self.restore_saved(self.generation()).await;
        }
        let credentials = self.credentials()?;
        let previous = { self.inner.state.lock().await.tokens.clone() };
        let Some(previous) = previous else {
            return Ok(());
        };
        if previous.expires_at <= identity::now_ms() + 60_000 {
            let tokens = self.oidc()?.refresh(&previous).await?;
            self.check_credentials(&credentials)?;
            let secrets = self.inner.state.lock().await.secrets.clone();
            let saved = Zeroizing::new(serde_json::to_string(&tokens)?);
            secrets
                .run(credentials.cancel.clone(), move |store| {
                    store.store("tokens", &saved)
                })
                .await?;
            self.check_credentials(&credentials)?;
            self.inner.state.lock().await.tokens = Some(tokens.clone());
            if let Some(credentials) = self
                .inner
                .credentials
                .write()
                .map_err(|_| Error::Closed)?
                .as_mut()
            {
                credentials.token = Zeroizing::new(tokens.access_token.clone());
            }
        }
        if let Err(error) = self.ensure_enrolled().await {
            self.suspend_transports().await;
            return Err(error);
        }
        self.check_credentials(&credentials)?;
        self.reconcile_device_removals().await?;
        self.reconcile_shares().await?;
        self.poll_link().await?;
        Ok(())
    }

    pub(super) async fn suspend_transports(&self) {
        for publication in self.inner.publications.lock().await.values() {
            publication.invalidate().await;
        }
        for cancel in self.inner.connections.lock().await.drain(..) {
            cancel.cancel();
        }
    }

    pub(super) async fn cancel_transports(&self) -> BTreeMap<Uuid, Arc<relay::Publication>> {
        let notification = self.inner.notifications.lock().await.take();
        if let Some(cancel) = notification {
            cancel.cancel();
        }
        let publications = std::mem::take(&mut *self.inner.publications.lock().await);
        for publication in publications.values() {
            publication.cancel.cancel();
            publication.invalidate().await;
        }
        for cancel in self.inner.connections.lock().await.drain(..) {
            cancel.cancel();
        }
        publications
    }

    pub(super) async fn retire_account(&self, erase_tokens: bool) -> Result<()> {
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
        state.tokens = None;
        state.link = None;
        let secrets = state.secrets.clone();
        drop(state);
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
        let publications = self.cancel_transports().await;
        if let Some(mut credentials) = cleanup {
            credentials.cancel = CancellationToken::new();
            self.delete_publications(&credentials, publications).await;
        }
        *self.inner.identity.write().map_err(|_| Error::Closed)? = None;
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = None;
        result
    }

    pub(super) async fn delete_publications(
        &self,
        credentials: &Credentials,
        publications: BTreeMap<Uuid, Arc<relay::Publication>>,
    ) {
        use futures_util::{StreamExt as _, stream};
        let pending=stream::iter(publications.into_values()).map(|publication|async move {
            let info=publication.info.read().await.clone();
            self.inner.http.device::<Value>(Method::DELETE,&format!("api/sessions/{}?incarnationId={}",info.session_id,info.incarnation_id),credentials,None).await
        }).buffer_unordered(8).for_each(|result|async move {
            if let Err(error)=result {
                self.emit_for(credentials.generation,Some(credentials.user_id.clone()),json!({"type":"system.error","message":format!("Remote publication cleanup was not confirmed: {error}")}));
            }
        });
        if tokio::time::timeout(Duration::from_secs(6), pending)
            .await
            .is_err()
        {
            self.emit_for(credentials.generation,Some(credentials.user_id.clone()),json!({"type":"system.error","message":"Remote publication cleanup timed out; offline metadata will expire."}));
        }
    }

    pub(super) async fn logout(&self) -> Result<()> {
        self.retire_account(true).await
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
