use super::*;

impl BackendClient {
    pub(super) async fn device_events(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let verified = self
            .fetch_identity_with(&credentials, &credentials.user_id, true)
            .await?;
        let devices=verified.devices.values().map(|cert|json!({"deviceId":cert.device_id,"label":cert.device_label,"certSignerDeviceId":cert.signer_device_id,"certIssuedAtMs":cert.issued_at_ms})).collect::<Vec<_>>();
        let mut result = vec![
            json!({"type":"devices.list","selfDeviceId":credentials.keys.device_id,"localDeviceEnrolled":credentials.enrolled,"devices":devices}),
        ];
        if credentials.enrolled {
            let requests: Value = self
                .inner
                .http
                .device(Method::GET, "api/devices/link/requests", &credentials, None)
                .await?;
            result.push(json!({"type":"devices.link.snapshot","requests":requests}));
        }
        Ok(result)
    }

    async fn reapproval_credentials(&self) -> Result<Credentials> {
        let credentials = self.credentials()?;
        if credentials.enrolled {
            return Ok(credentials);
        }
        self.fetch_identity_with(&credentials, &credentials.user_id, false)
            .await?;
        let mut state = self.inner.state.lock().await;
        if !state
            .pins
            .lock()
            .map_err(|_| Error::Closed)?
            .is_revoked(&credentials.user_id, &credentials.keys.device_id)
        {
            return Ok(credentials);
        }
        state.link = None;
        drop(state);
        self.replace_local_keys(credentials).await
    }

    pub(super) async fn replace_local_keys(
        &self,
        mut credentials: Credentials,
    ) -> Result<Credentials> {
        let secrets = self.inner.state.lock().await.secrets.clone();
        let user = credentials.user_id.clone();
        let previous = credentials.keys.device_id.clone();
        credentials.keys = Arc::new(
            secrets
                .run(credentials.cancel.clone(), move |store| {
                    DeviceKeys::replace_revoked(store, &user, &previous)
                })
                .await?,
        );
        self.check_credentials(&credentials)?;
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(credentials.clone());
        if let Some(identity) = self
            .inner
            .identity
            .write()
            .map_err(|_| Error::Closed)?
            .as_mut()
        {
            identity.device_id.clone_from(&credentials.keys.device_id);
            identity.enrolled = false;
        }
        Ok(credentials)
    }

    pub(super) async fn reset_devices(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        if credentials.enrolled {
            return Err(invalid(
                "This device is already trusted. Remove other devices from Settings instead.",
            ));
        }
        let _response: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/me/identity/reset",
                &credentials.token,
                None,
            )
            .await?;
        let pins = {
            let mut state = self.inner.state.lock().await;
            state.link = None;
            Arc::clone(&state.pins)
        };
        pins.lock()
            .map_err(|_| Error::Closed)?
            .forget(&credentials.user_id)?;
        let credentials = self.replace_local_keys(credentials).await?;
        self.ensure_enrolled().await?;
        if !self.identity().is_some_and(|identity| identity.enrolled) {
            return Err(invalid("This device could not be trusted after the reset."));
        }
        let mut events =
            vec![json!({"type":"auth.ready","userId":credentials.user_id,"enrolled":true})];
        events.extend(self.device_events().await?);
        events.push(self.session_event().await?);
        Ok(events)
    }

    pub(super) async fn start_link(&self) -> Result<Value> {
        let credentials = self.reapproval_credentials().await?;
        if credentials.enrolled {
            return Err(invalid("This device is already approved."));
        }
        let link:Value=self.inner.http.bearer(Method::POST,"api/devices/link/init",&credentials.token,Some(json!({"deviceId":credentials.keys.device_id,"deviceLabel":host_label(),"kemPublicKey":BASE64.encode(credentials.keys.kem_public()),"signingPublicKey":BASE64.encode(credentials.keys.signing_public())}))).await?;
        let result = json!({"type":"devices.link.selfPending","userCode":link["userCode"],"expiresAt":link["expiresAt"]});
        self.inner.state.lock().await.link = Some(link);
        Ok(result)
    }

    pub(super) async fn cancel_link(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let link = self.inner.state.lock().await.link.clone();
        if let Some(link) = link {
            let code = wire::text(&link, "userCode")?;
            let _response: Value = self
                .inner
                .http
                .bearer(
                    Method::DELETE,
                    &format!("api/devices/link/requests/{}", path_segment(code)?),
                    &credentials.token,
                    None,
                )
                .await?;
            self.inner.state.lock().await.link = None;
        }
        Ok(())
    }

    pub(super) async fn poll_link(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let link = self.inner.state.lock().await.link.clone();
        let Some(link) = link else {
            return Ok(());
        };
        let code = wire::text(&link, "deviceCode")?;
        let result: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/devices/link/poll",
                &credentials.token,
                Some(json!({"deviceCode":code})),
            )
            .await?;
        match wire::text(&result, "state")? {
            "pending" => Ok(()),
            "approved" => {
                let verified = self
                    .fetch_identity_with(&credentials, &credentials.user_id, true)
                    .await?;
                let cert = verified
                    .devices
                    .get(&credentials.keys.device_id)
                    .ok_or_else(|| invalid("Approved device is absent from the signed list."))?;
                if cert.sig_public_key != credentials.keys.signing_public()
                    || cert.kem_public_key != credentials.keys.kem_public()
                {
                    return Err(Error::Trust(
                        "Approval contained another device's keys.".into(),
                    ));
                }
                let _response: Value = self
                    .inner
                    .http
                    .bearer(
                        Method::POST,
                        "api/devices/link/ack",
                        &credentials.token,
                        Some(json!({"deviceCode":code,"deviceId":credentials.keys.device_id})),
                    )
                    .await?;
                self.inner.state.lock().await.link = None;
                self.ensure_enrolled().await?;
                if !self.identity().is_some_and(|identity| identity.enrolled) {
                    return Err(invalid("Device approval has not become current."));
                }
                self.emit_for(
                    credentials.generation,
                    Some(credentials.user_id.clone()),
                    json!({"type":"devices.link.selfResolved","outcome":"approved"}),
                );
                self.emit_for(
                    credentials.generation,
                    Some(credentials.user_id.clone()),
                    json!({"type":"auth.ready","userId":credentials.user_id,"enrolled":true}),
                );
                for event in self.device_events().await? {
                    self.emit_for(
                        credentials.generation,
                        Some(credentials.user_id.clone()),
                        event,
                    );
                }
                Ok(())
            }
            state @ ("expired" | "cancelled") => {
                self.inner.state.lock().await.link = None;
                self.emit_for(
                    credentials.generation,
                    Some(credentials.user_id.clone()),
                    json!({"type":"devices.link.selfResolved","outcome":state}),
                );
                Ok(())
            }
            _ => Err(invalid("Unsupported device approval result.")),
        }
    }

    pub(super) async fn approve_link(&self, code: &str) -> Result<()> {
        let credentials = self.credentials()?;
        let pending: Value = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/devices/link/pending?userCode={}", path_segment(code)?),
                &credentials,
                None,
            )
            .await?;
        let verified = self.fetch_identity(&credentials.user_id, false).await?;
        let new_device = wire::text(&pending, "deviceId")?;
        if verified.devices.contains_key(new_device) {
            return Err(invalid("This device is already approved."));
        }
        let issued = verified.list.successor_issued_at(identity::now_ms())?;
        let cert = build_cert_for(
            &credentials.user_id,
            new_device,
            wire::text(&pending, "deviceLabel")?,
            &wire::decode_b64(&pending, "kemPublicKey", 1184)?,
            &wire::decode_b64(&pending, "signingPublicKey", 1952)?,
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
            issued,
            None,
        )?;
        let mut entries = verified.list.entries.clone();
        entries.push(DeviceListEntry {
            device_id: new_device.to_owned(),
            signer_device_id: credentials.keys.device_id.clone(),
        });
        let list = build_replacement_list(
            &credentials.user_id,
            verified.generation,
            &verified.list.entries,
            entries,
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
            issued,
            Some(issued + 24 * 60 * 60_000),
        )?;
        let _response:Value=self.inner.http.device(Method::POST,"api/devices/link/approve",&credentials,Some(json!({"userCode":code,"deviceCertificate":BASE64.encode(cert.body_bytes),"deviceCertificateSignature":BASE64.encode(cert.signature),"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature)}))).await?;
        self.fetch_identity(&credentials.user_id, false).await?;
        self.rekey_all().await
    }

    pub(super) async fn reconcile_device_removals(&self) -> Result<()> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Ok(());
        }
        let pending = self
            .inner
            .state
            .lock()
            .await
            .blocked_devices
            .iter()
            .filter(|(user, _)| user == &credentials.user_id)
            .map(|(_, device)| device.clone())
            .collect::<Vec<_>>();
        for device in pending {
            self.revoke_device(&device).await?;
        }
        Ok(())
    }

    pub(super) async fn revoke_device(&self, id: &str) -> Result<()> {
        let credentials = self.credentials()?;
        if id == credentials.keys.device_id {
            return Err(invalid("Remove this device from another approved device."));
        }
        let verified = self.fetch_identity(&credentials.user_id, false).await?;
        if !verified
            .list
            .entries
            .iter()
            .any(|entry| entry.device_id == id)
        {
            return Ok(());
        }
        let entries = verified
            .list
            .entries
            .iter()
            .filter(|entry| entry.device_id != id)
            .cloned()
            .collect();
        let issued = verified.list.successor_issued_at(identity::now_ms())?;
        let list = build_replacement_list(
            &credentials.user_id,
            verified.generation,
            &verified.list.entries,
            entries,
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
            issued,
            Some(issued + 24 * 60 * 60_000),
        )?;
        self.block_device(&credentials.user_id, id).await?;
        self.rekey_all().await?;
        let _response:Value=self.inner.http.device(Method::POST,"api/me/identity/device-list",&credentials,Some(json!({"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature)}))).await?;
        self.fetch_identity(&credentials.user_id, false).await?;
        self.rekey_all().await
    }
}
