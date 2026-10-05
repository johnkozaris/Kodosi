use std::time::Instant;

use serde::Deserialize;

use super::*;
use crate::identity::{
    ML_DSA_65_PUBLIC_KEY_LEN,
    link_code::{LinkIdentity, LinkKey, NONCE_LEN, PROOF_LEN, new_code, typed_code},
    pins::decode,
};

const LINK_REQUESTS: usize = 5;
const LINK_LIFE: Duration = Duration::from_mins(10);
const LINK_POLL: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub(crate) struct PendingLink {
    device_code: String,
    request_id: String,
    key: Arc<LinkKey>,
    started: Instant,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinkRequest {
    request_id: Uuid,
    device_id: String,
    device_label: String,
    signing_public_key: String,
    nonce: String,
    proof: String,
    expires_at: String,
}

pub(super) struct LinkedDevice {
    request_id: Uuid,
    pub(super) device_id: String,
    label: String,
    signing_public_key: Vec<u8>,
    key: LinkKey,
}

pub(super) fn request_with_code(
    user_id: &str,
    requests: Vec<LinkRequest>,
    code: &str,
) -> Result<Option<LinkedDevice>> {
    for request in requests.into_iter().take(LINK_REQUESTS) {
        let signing_public_key = decode(&request.signing_public_key, ML_DSA_65_PUBLIC_KEY_LEN)?;
        let key = LinkKey::derive(code, &decode(&request.nonce, NONCE_LEN)?);
        let identity = LinkIdentity {
            user_id,
            device_id: &request.device_id,
            label: &request.device_label,
            signing_public_key: &signing_public_key,
        };
        if key.proves_request(&identity, &decode(&request.proof, PROOF_LEN)?)? {
            return Ok(Some(LinkedDevice {
                request_id: request.request_id,
                device_id: request.device_id,
                label: request.device_label,
                signing_public_key,
                key,
            }));
        }
    }
    Ok(None)
}

impl BackendClient {
    pub(super) async fn enrollment_events(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let mut events = vec![
            json!({"type":"auth.ready","userId":credentials.user_id,"enrolled":credentials.enrolled}),
        ];
        events.extend(self.device_events().await?);
        Ok(events)
    }

    pub(super) async fn device_events(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let devices = if self.pinned(&credentials.user_id).await? {
            match self
                .fetch_identity_with(&credentials, &credentials.user_id)
                .await
            {
                Ok(verified) => verified.devices.values().map(|cert|json!({"deviceId":cert.device_id,"label":cert.device_label,"certSignerDeviceId":cert.signer_device_id,"certIssuedAtMs":cert.issued_at_ms})).collect(),
                Err(Error::Invalid { .. } | Error::Trust(_) | Error::Backend { status: 404, .. })
                    if !credentials.enrolled =>
                {
                    Vec::new()
                }
                Err(error) => return Err(error),
            }
        } else {
            Vec::new()
        };
        let mut result = vec![
            json!({"type":"devices.list","selfDeviceId":credentials.keys.device_id,"localDeviceEnrolled":credentials.enrolled,"notice":credentials.notice,"devices":devices}),
        ];
        if credentials.enrolled {
            let requests = self
                .link_requests(&credentials)
                .await?
                .into_iter()
                .map(|request| json!({"requestId":request.request_id,"deviceLabel":request.device_label,"expiresAt":request.expires_at}))
                .collect::<Vec<_>>();
            result.push(json!({"type":"devices.link.snapshot","requests":requests}));
        }
        Ok(result)
    }

    async fn reapproval_credentials(&self) -> Result<Credentials> {
        let credentials = self.credentials()?;
        if credentials.enrolled || !self.pinned(&credentials.user_id).await? {
            return Ok(credentials);
        }
        match self
            .fetch_identity_with(&credentials, &credentials.user_id)
            .await
        {
            Err(Error::Invalid { .. } | Error::Trust(_) | Error::Backend { status: 404, .. }) => {
                return Err(invalid(
                    "No device of this account can approve this device now. Start fresh on this device.",
                ));
            }
            result => result?,
        };
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
        let label = host_label();
        let mut nonce = [0; NONCE_LEN];
        aws_lc_rs::rand::fill(&mut nonce).map_err(|_| Error::Closed)?;
        let code = new_code()?;
        let (user, keys, secret, name) = (
            credentials.user_id.clone(),
            Arc::clone(&credentials.keys),
            code.clone(),
            label.clone(),
        );
        let (key, proof) = tokio::task::spawn_blocking(move || {
            let key = LinkKey::derive(&secret, &nonce);
            let proof = key.request_proof(&LinkIdentity {
                user_id: &user,
                device_id: &keys.device_id,
                label: &name,
                signing_public_key: keys.signing_public(),
            })?;
            Ok::<_, Error>((key, proof))
        })
        .await
        .map_err(|_| Error::Closed)??;
        let link:Value=self.inner.http.bearer(Method::POST,"api/devices/link/init",&credentials.token,Some(json!({"deviceId":credentials.keys.device_id,"deviceLabel":label,"signingPublicKey":BASE64.encode(credentials.keys.signing_public()),"nonce":BASE64.encode(nonce),"proof":BASE64.encode(proof)}))).await?;
        let result =
            json!({"type":"devices.link.selfPending","code":code,"expiresAt":link["expiresAt"]});
        self.inner.state.lock().await.link = Some(PendingLink {
            device_code: wire::text(&link, "deviceCode")?.to_owned(),
            request_id: wire::text(&link, "requestId")?.to_owned(),
            key: Arc::new(key),
            started: Instant::now(),
        });
        Ok(result)
    }

    async fn link_requests(&self, credentials: &Credentials) -> Result<Vec<LinkRequest>> {
        self.inner
            .http
            .device(Method::GET, "api/devices/link/requests", credentials, None)
            .await
    }

    pub(super) async fn cancel_link(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let link = self.inner.state.lock().await.link.clone();
        if let Some(link) = link {
            let _response: Value = self
                .inner
                .http
                .bearer(
                    Method::DELETE,
                    &format!(
                        "api/devices/link/requests/{}",
                        path_segment(&link.request_id)?
                    ),
                    &credentials.token,
                    None,
                )
                .await?;
            self.inner.state.lock().await.link = None;
        }
        Ok(())
    }

    pub(super) async fn watch_link(&self) {
        let Some(watched) = self.inner.state.lock().await.link.clone() else {
            return;
        };
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = this.inner.shutdown.cancelled() => break,
                    () = tokio::time::sleep(LINK_POLL) => {}
                }
                let _operation = this.inner.operations.lock().await;
                let current = this.inner.state.lock().await.link.clone();
                if current.is_none_or(|link| link.request_id != watched.request_id) {
                    break;
                }
                if let Err(error) = this.poll_link().await
                    && error.user_facing()
                {
                    tracing::warn!(%error, "the device approval was not read; it will be tried again");
                }
            }
        });
    }

    async fn poll_link(&self) -> Result<()> {
        let credentials = self.credentials()?;
        let link = self.inner.state.lock().await.link.clone();
        let Some(link) = link else {
            return Ok(());
        };
        let resolved = |outcome: &str| {
            self.emit_for(
                credentials.generation,
                Some(credentials.user_id.clone()),
                json!({"type":"devices.link.selfResolved","outcome":outcome}),
            );
        };
        if link.started.elapsed() >= LINK_LIFE {
            self.inner.state.lock().await.link = None;
            resolved("expired");
            return Ok(());
        }
        let result: Value = self
            .inner
            .http
            .bearer(
                Method::POST,
                "api/devices/link/poll",
                &credentials.token,
                Some(json!({"deviceCode":link.device_code})),
            )
            .await?;
        match wire::text(&result, "state")? {
            "pending" => Ok(()),
            "approved" => {
                let accepted = self.accept_approval(&credentials, &link, &result).await;
                if matches!(accepted, Err(ref error) if error.unanswered()) {
                    return accepted;
                }
                self.inner.state.lock().await.link = None;
                if let Err(error) = accepted {
                    resolved("cancelled");
                    return Err(error);
                }
                self.ensure_enrolled().await?;
                if !self.identity().is_some_and(|identity| identity.enrolled) {
                    return Err(invalid("Device approval has not become current."));
                }
                resolved("approved");
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
                resolved(state);
                Ok(())
            }
            _ => Err(invalid("Unsupported device approval result.")),
        }
    }

    async fn accept_approval(
        &self,
        credentials: &Credentials,
        link: &PendingLink,
        result: &Value,
    ) -> Result<()> {
        let proof = wire::decode_b64(result, "approvalProof", PROOF_LEN)?;
        let bundle: IdentityBundle = self
            .inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await?;
        self.check_credentials(credentials)?;
        let root = bundle.root()?;
        if bundle.user_id != credentials.user_id
            || !link.key.proves_approval(
                &credentials.user_id,
                &credentials.keys.device_id,
                &root,
                &proof,
            )?
        {
            return Err(Error::Trust(
                "The approval did not come from a device that has your code. Request approval again.".into(),
            ));
        }
        let verified = self.verify_bundle(&bundle, Some(root)).await?;
        if !device_identity::device_enrolled(&verified, credentials) {
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
                Some(json!({"deviceCode":link.device_code,"deviceId":credentials.keys.device_id})),
            )
            .await?;
        Ok(())
    }

    pub(super) async fn approve_link(&self, typed: &str) -> Result<String> {
        let code = typed_code(typed).ok_or_else(|| {
            invalid("Type the 12 characters of the code that the new device shows.")
        })?;
        let credentials = self.credentials()?;
        let requests = self.link_requests(&credentials).await?;
        let user = credentials.user_id.clone();
        let pending =
            tokio::task::spawn_blocking(move || request_with_code(&user, requests, &code))
                .await
                .map_err(|_| Error::Closed)??
                .ok_or_else(|| {
                    Error::Trust(
                        "No device that waits for approval has this code. Compare it with the code on the new device.".into(),
                    )
                })?;
        let verified = self.fetch_identity(&credentials.user_id).await?;
        let new_device = pending.device_id.as_str();
        let approval =
            pending
                .key
                .approval_proof(&credentials.user_id, new_device, &verified.root)?;
        if verified.devices.contains_key(new_device) {
            return Err(invalid("This device is already approved."));
        }
        let issued = verified.list.successor_issued_at(identity::now_ms())?;
        let cert = build_cert_for(
            &credentials.user_id,
            new_device,
            &pending.label,
            &pending.signing_public_key,
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
        let _response:Value=self.inner.http.device(Method::POST,"api/devices/link/approve",&credentials,Some(json!({"requestId":pending.request_id,"deviceCertificate":BASE64.encode(cert.body_bytes),"deviceCertificateSignature":BASE64.encode(cert.signature),"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature),"approvalProof":BASE64.encode(approval)}))).await?;
        self.fetch_identity(&credentials.user_id).await?;
        self.review_access().await?;
        Ok(pending.label)
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
        if let Err(error) = self.resign_friends(&credentials).await {
            tracing::warn!(%error, "the friend list keeps the signature of the removed device");
        }
        let verified = self.fetch_identity(&credentials.user_id).await?;
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
        self.review_access().await?;
        let _response:Value=self.inner.http.device(Method::POST,"api/me/identity/device-list",&credentials,Some(json!({"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature)}))).await?;
        self.fetch_identity(&credentials.user_id).await?;
        self.review_access().await
    }
}
