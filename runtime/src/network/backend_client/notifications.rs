use super::{
    BackendClient, CancellationToken, Credentials, Duration, Error, Result, invalid,
    terminal_connections,
};

impl BackendClient {
    pub(super) async fn start_notifications(&self) -> Result<()> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Ok(());
        }
        let mut current = self.inner.notifications.lock().await;
        if current
            .as_ref()
            .is_some_and(|cancel| !cancel.is_cancelled())
        {
            return Ok(());
        }
        let cancel = credentials.cancel.child_token();
        *current = Some(cancel.clone());
        drop(current);
        let network = self.clone();
        tokio::spawn(async move {
            let mut delay = 1;
            loop {
                if cancel.is_cancelled() || network.check_credentials(&credentials).is_err() {
                    break;
                }
                let started = tokio::time::Instant::now();
                let result = tokio::select! {
                    biased;
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    result=network.notification_connection(&credentials,&cancel)=>result,
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "notification stream interrupted; reconnecting");
                }
                if started.elapsed() >= terminal_connections::STABLE_CONNECTION {
                    delay = 1;
                }
                tokio::select! {
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    ()=tokio::time::sleep(Duration::from_secs(delay))=>{},
                }
                delay = (delay * 2).min(15);
            }
            cancel.cancel();
        });
        Ok(())
    }

    async fn notification_connection(
        &self,
        admitted: &Credentials,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let credentials = self.credentials()?;
        self.check_credentials(admitted)?;
        let (surfaces, mut changed) = tokio::sync::mpsc::unbounded_channel::<String>();
        let refresh = async {
            while let Some(surface) = changed.recv().await {
                let mut waiting = std::collections::BTreeSet::from([surface]);
                while let Ok(surface) = changed.try_recv() {
                    waiting.insert(surface);
                }
                for surface in ["devices", "friends", "sessions", "missions"] {
                    if waiting.contains(surface) {
                        self.refresh_surface(admitted, surface).await?;
                    }
                }
                if waiting.iter().any(|surface| {
                    !["devices", "friends", "sessions", "missions"].contains(&surface.as_str())
                }) {
                    return Err(invalid("Unsupported notification surface."));
                }
            }
            Ok(())
        };
        let result = tokio::select! {
            biased;
            () = cancel.cancelled() => Ok(()),
            () = self.inner.shutdown.cancelled() => Ok(()),
            result = terminal_connections::run_link(self, &credentials, &surfaces) => result,
            result = refresh => result,
        };
        if matches!(result, Err(Error::Backend { status: 403, .. })) {
            self.refresh_surface(admitted, "devices").await?;
        }
        result
    }

    async fn validate_notified_device(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let verified = self
            .fetch_identity_with(&credentials, &credentials.user_id)
            .await?;
        let active = verified
            .devices
            .get(&credentials.keys.device_id)
            .is_some_and(|cert| {
                cert.sig_public_key == credentials.keys.signing_public()
                    && cert.kem_public_key == credentials.keys.kem_public()
            });
        if !active {
            self.suspend_transports().await;
            let mut next = credentials.clone();
            next.enrolled = false;
            *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(next);
            if let Some(identity) = self
                .inner
                .identity
                .write()
                .map_err(|_| Error::Closed)?
                .as_mut()
            {
                identity.enrolled = false;
            }
            return Err(Error::EnrollmentRequired);
        }
        Ok(())
    }

    async fn refresh_surface(&self, admitted: &Credentials, surface: &str) -> Result<()> {
        let _operation = self.inner.operations.lock().await;
        self.check_credentials(admitted)?;
        let events = match surface {
            "sessions" => vec![self.session_event().await?],
            "friends" => vec![self.friend_event().await?],
            "devices" => {
                self.validate_notified_device().await?;
                self.device_events().await?
            }
            "missions" => vec![self.mission_list().await?],
            _ => return Err(invalid("Unsupported notification surface.")),
        };
        for event in events {
            self.emit_for(admitted.generation, Some(admitted.user_id.clone()), event);
        }
        Ok(())
    }
}
