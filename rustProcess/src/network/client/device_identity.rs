use super::*;

impl Network {
    pub(super) async fn bootstrap_identity(
        &self,
        credentials: &Credentials,
    ) -> Result<IdentityBundle> {
        if self
            .inner
            .state
            .lock()
            .await
            .pins
            .lock()
            .map_err(|_| Error::Closed)?
            .contains(&credentials.user_id)
        {
            return Err(Error::Trust(
                "The server lost a previously trusted identity; refusing to replace it.".into(),
            ));
        }
        let now = identity::now_ms();
        let cert = build_self_cert(
            &credentials.user_id,
            &credentials.keys.device_id,
            &host_label(),
            credentials.keys.kem_public(),
            &credentials.keys.signing_key()?,
            now,
            None,
        )?;
        let list = build_bootstrap_list(
            &credentials.user_id,
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
            now,
            Some(now + 24 * 60 * 60_000),
        )?;
        let challenge: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/me/devices/challenge",
                &credentials.token,
                None,
            )
            .await?;
        let challenge_bytes = wire::decode_b64(&challenge, "challengeBytes", 32)?;
        let signature =
            crypto::sign_pop_challenge(&credentials.keys.signing_key()?, &challenge_bytes)?;
        let _response:Value=self.inner.http.bearer(Method::POST,"api/me/devices",&credentials.token,Some(json!({
        "deviceId":credentials.keys.device_id,"kemPublicKey":BASE64.encode(credentials.keys.kem_public()),"signingPublicKey":BASE64.encode(credentials.keys.signing_public()),
        "challengeId":challenge["challengeId"],"popSignature":BASE64.encode(signature),"deviceCertificate":BASE64.encode(cert.body_bytes),"deviceCertificateSignature":BASE64.encode(cert.signature),
        "signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature),
    }))).await?;
        self.inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await
    }

    pub(super) async fn ensure_enrolled(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let result = self
            .inner
            .http
            .bearer::<IdentityBundle>(Method::GET, "api/me/identity", &credentials.token, None)
            .await;
        let bundle = match result {
            Ok(bundle) => bundle,
            Err(Error::Backend { status: 404, .. }) => {
                self.bootstrap_identity(&credentials).await?
            }
            Err(error) => return Err(error),
        };
        self.check_credentials(&credentials)?;
        if bundle.user_id != credentials.user_id {
            return Err(Error::Trust(
                "Device identity belongs to another account.".into(),
            ));
        }
        let now = identity::now_ms();
        let pins = Arc::clone(&self.inner.state.lock().await.pins);
        let identity_bundle = bundle.clone();
        let verified_user = credentials.user_id.clone();
        let mut verified = tokio::task::spawn_blocking(move || {
            let mut pins = pins.lock().map_err(|_| Error::Closed)?;
            if pins.contains(&verified_user) {
                pins.verify_for_renewal(&identity_bundle, now)
            } else {
                pins.verify(&identity_bundle, true, now)
            }
        })
        .await
        .map_err(|_| Error::Closed)??;
        let enrolled = verified
            .devices
            .get(&credentials.keys.device_id)
            .is_some_and(|cert| {
                cert.sig_public_key == credentials.keys.signing_public()
                    && cert.kem_public_key == credentials.keys.kem_public()
            });
        if !enrolled {
            let notification = self.inner.notifications.lock().await.take();
            if let Some(cancel) = notification {
                cancel.cancel();
            }
        }
        if !enrolled && credentials.enrolled {
            self.suspend_transports().await;
        }
        if enrolled
            && verified
                .list
                .expires_at_ms
                .is_some_and(|expiry| expiry <= now + 6 * 60 * 60_000)
        {
            self.renew_device_list(&credentials, &verified, now).await?;
            verified = self.fetch_identity(&credentials.user_id, false).await?;
        }
        if verified
            .list
            .expires_at_ms
            .is_some_and(|expiry| expiry <= now)
        {
            return Err(Error::Trust(
                "The signed device list needs renewal from an approved device.".into(),
            ));
        }
        let enrolled = verified
            .devices
            .get(&credentials.keys.device_id)
            .is_some_and(|cert| {
                cert.sig_public_key == credentials.keys.signing_public()
                    && cert.kem_public_key == credentials.keys.kem_public()
            });
        self.check_credentials(&credentials)?;
        let mut next = credentials.clone();
        next.enrolled = enrolled;
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(next);
        if let Some(identity) = self
            .inner
            .identity
            .write()
            .map_err(|_| Error::Closed)?
            .as_mut()
        {
            identity.enrolled = enrolled;
        }
        if enrolled {
            self.start_notifications().await?;
        }
        if !enrolled {
            self.emit_for(credentials.generation,Some(credentials.user_id),json!({"type":"auth.notice","message":"Approve this device from one of your existing devices."}));
        }
        Ok(())
    }

    pub(super) async fn renew_device_list(
        &self,
        credentials: &Credentials,
        verified: &VerifiedIdentity,
        now: u64,
    ) -> Result<()> {
        let issued = now.max(verified.list.issued_at_ms.saturating_add(1));
        let list = build_replacement_list(
            &credentials.user_id,
            verified.generation,
            &verified.list.entries,
            verified.list.entries.clone(),
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
            issued,
            Some(issued + 24 * 60 * 60_000),
        )?;
        let mut renewal = credentials.clone();
        renewal.enrolled = true;
        let result=self.inner.http.device::<Value>(Method::POST,"api/me/identity/device-list",&renewal,Some(json!({"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature)}))).await;
        if let Err(error) = result
            && !matches!(error, Error::Backend { status: 409, .. })
        {
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn verify_bundle(
        &self,
        bundle: &IdentityBundle,
        allow_first: bool,
    ) -> Result<VerifiedIdentity> {
        let pins = Arc::clone(&self.inner.state.lock().await.pins);
        let bundle = bundle.clone();
        tokio::task::spawn_blocking(move || {
            pins.lock()
                .map_err(|_| Error::Closed)?
                .verify(&bundle, allow_first, identity::now_ms())
        })
        .await
        .map_err(|_| Error::Closed)?
    }

    pub(crate) async fn fetch_identity(
        &self,
        user_id: &str,
        allow_first: bool,
    ) -> Result<VerifiedIdentity> {
        let credentials = self.credentials()?;
        self.fetch_identity_with(&credentials, user_id, allow_first)
            .await
    }

    pub(crate) async fn fetch_identity_with(
        &self,
        credentials: &Credentials,
        user_id: &str,
        allow_first: bool,
    ) -> Result<VerifiedIdentity> {
        self.check_credentials(credentials)?;
        let bundle: IdentityBundle = if user_id == credentials.user_id {
            self.inner
                .http
                .bearer(Method::GET, "api/me/identity", &credentials.token, None)
                .await?
        } else {
            self.inner
                .http
                .device(
                    Method::GET,
                    &format!("api/users/{}/identity", path_segment(user_id)?),
                    credentials,
                    None,
                )
                .await?
        };
        self.check_credentials(credentials)?;
        if bundle.user_id != user_id {
            return Err(Error::Trust(
                "Returned device identity belongs to another account.".into(),
            ));
        }
        let mut verified = self.verify_bundle(&bundle, allow_first).await?;
        let mut state = self.inner.state.lock().await;
        let mut pending = state.blocked_devices.clone();
        pending.retain(|(user, device)| {
            user != user_id
                || verified
                    .list
                    .entries
                    .iter()
                    .any(|entry| &entry.device_id == device)
        });
        if pending != state.blocked_devices {
            identity::storage::private_write(
                &self
                    .inner
                    .config
                    .data_root
                    .join("network/pending-device-removals.json"),
                &serde_json::to_vec(&pending)?,
            )?;
            state.blocked_devices = pending;
        }
        verified.devices.retain(|device, _| {
            !state
                .blocked_devices
                .contains(&(user_id.to_owned(), device.clone()))
        });
        drop(state);
        Ok(verified)
    }

    pub(super) async fn block_device(&self, user_id: &str, device_id: &str) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let mut pending = state.blocked_devices.clone();
        if pending.len() >= 256 && !pending.contains(&(user_id.to_owned(), device_id.to_owned())) {
            return Err(Error::Busy);
        }
        pending.insert((user_id.to_owned(), device_id.to_owned()));
        identity::storage::private_write(
            &self
                .inner
                .config
                .data_root
                .join("network/pending-device-removals.json"),
            &serde_json::to_vec(&pending)?,
        )?;
        state.blocked_devices = pending;
        drop(state);
        Ok(())
    }
}
