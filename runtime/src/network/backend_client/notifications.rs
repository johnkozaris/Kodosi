use super::{
    BackendClient, CancellationToken, Credentials, Duration, Error, Result, Value, invalid, json,
    terminal_connections, wire,
};
use futures_util::StreamExt as _;
use tokio_tungstenite::tungstenite::Message;

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
                let result = tokio::select! {
                    biased;
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    result=network.notification_connection(&credentials,&cancel)=>result,
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "notification stream interrupted; reconnecting");
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
        let (mut socket, _) =
            terminal_connections::socket(self, &credentials, "ws/events", "events", None, None)
                .await?;
        for surface in ["sessions", "friends", "devices", "missions"] {
            self.refresh_surface(admitted, surface).await?;
        }
        let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                ()=cancel.cancelled()=>return Ok(()),
                ()=self.inner.shutdown.cancelled()=>return Ok(()),
                _=heartbeat.tick()=>terminal_connections::send_json(&mut socket,json!({"type":"ping"})).await?,
                incoming=socket.next()=>{
                    let message=incoming.ok_or(Error::Closed)?.map_err(|error|invalid(error.to_string()))?;
                    match message {
                        Message::Text(text)=>{
                            if text.len()>16*1024{return Err(invalid("Notification exceeds its size limit."));}
                            let value:Value=serde_json::from_str(&text)?;
                            match wire::text(&value,"type")? {
                                "ping"=>terminal_connections::send_json(&mut socket,json!({"type":"pong"})).await?,
                                "pong"=>{},
                                "changed"=>self.refresh_surface(admitted,wire::text(&value,"surface")?).await?,
                                _=>return Err(invalid("Unsupported account notification.")),
                            }
                        }
                        Message::Ping(payload)=>terminal_connections::send_pong(&mut socket,payload).await?,
                        Message::Pong(_)=>{},
                        Message::Close(_)=>return Err(Error::Closed),
                        _=>return Err(invalid("Unexpected notification payload.")),
                    }
                }
            }
        }
    }

    async fn validate_notified_device(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let verified = self
            .fetch_identity_with(&credentials, &credentials.user_id, false)
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
