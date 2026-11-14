use std::time::Instant;

use serde::Deserialize;

use super::*;
use crate::identity::{
    ML_DSA_65_PUBLIC_KEY_LEN,
    link_code::{
        LinkIdentity, LinkKey, LinkSecret, NONCE_LEN, PROOF_LEN, ScannedLink, new_code,
        scanned_link, typed_code,
    },
    pins::decode,
};

const LINK_REQUESTS: usize = 5;
pub(super) const LINK_LIFE: Duration = Duration::from_mins(10);
const LINK_POLL: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub(crate) struct PendingLink {
    pub(super) device_code: String,
    pub(super) request_id: String,
    pub(super) key: Arc<LinkKey>,
    pub(super) started: Instant,
    pub(super) accepted: bool,
}

#[derive(Clone)]
pub(crate) struct PendingOffer {
    secret: Arc<LinkSecret>,
    started: tokio::time::Instant,
    refused: BTreeSet<String>,
}

pub(super) struct ShownCode {
    pub(super) code: String,
    pub(super) link: String,
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingReset {
    request_id: String,
    device_id: String,
    device_label: String,
    requested_at_ms: i64,
    ready_at_ms: i64,
    ready: bool,
}

pub(super) struct LinkedDevice {
    request_id: Uuid,
    pub(super) device_id: String,
    label: String,
    signing_public_key: Vec<u8>,
    key: LinkKey,
}

pub(super) fn request_with_secret<'a>(
    user_id: &str,
    requests: impl IntoIterator<Item = &'a LinkRequest>,
    secret: &LinkSecret,
) -> Result<Option<LinkedDevice>> {
    for request in requests.into_iter().take(LINK_REQUESTS) {
        let signing_public_key = decode(&request.signing_public_key, ML_DSA_65_PUBLIC_KEY_LEN)?;
        let key = LinkKey::derive(secret, &decode(&request.nonce, NONCE_LEN)?)?;
        let identity = LinkIdentity {
            user_id,
            device_id: &request.device_id,
            label: &request.device_label,
            signing_public_key: &signing_public_key,
        };
        if key.proves_request(&identity, &decode(&request.proof, PROOF_LEN)?)? {
            return Ok(Some(LinkedDevice {
                request_id: request.request_id,
                device_id: request.device_id.clone(),
                label: request.device_label.clone(),
                signing_public_key,
                key,
            }));
        }
    }
    Ok(None)
}

fn another_account() -> Error {
    Error::Text(text!(
        "This code is from another account. Sign in to the same account on both devices."
    ))
}

impl BackendClient {
    pub(super) async fn enrollment_events(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let mut events = vec![self.ready_event(&credentials.user_id, credentials.enrolled)];
        events.extend(self.device_events().await?);
        Ok(events)
    }

    pub(super) async fn device_events(&self) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let offered = self.settle_offer(&credentials).await;
        let devices = if self.pinned(&credentials.user_id).await? {
            match self
                .fetch_identity_with(&credentials, &credentials.user_id)
                .await
            {
                Ok(verified) => verified.devices.values().map(|cert|json!({"deviceId":cert.device_id,"label":cert.device_label,"certIssuedAtMs":cert.issued_at_ms,"recoveryKey":identity::recovery::is_recovery_device(&cert.device_id)})).collect(),
                Err(Error::Invalid { .. } | Error::Trust(_) | Error::Backend { status: 404, .. })
                    if !credentials.enrolled =>
                {
                    Vec::new()
                }
                Err(Error::Trust(reason)) => {
                    if self.settle_enrollment().await? {
                        return Err(Error::Trust(reason));
                    }
                    return Box::pin(self.enrollment_events()).await;
                }
                Err(error) => return Err(error),
            }
        } else {
            Vec::new()
        };
        let reset = self.pending_reset(&credentials).await?.map(|reset| json!({"resetId":reset.request_id,"deviceLabel":reset.device_label,"thisDevice":reset.device_id == credentials.keys.device_id,"requestedAtMs":reset.requested_at_ms,"readyAtMs":reset.ready_at_ms}));
        let mut result = vec![
            json!({"type":"devices.list","selfDeviceId":credentials.keys.device_id,"localDeviceEnrolled":credentials.enrolled,"notice":credentials.notice,"devices":devices,"reset":reset}),
        ];
        if credentials.enrolled {
            let requests = self
                .link_requests(&credentials)
                .await?
                .into_iter()
                .map(|request| json!({"requestId":request.request_id,"deviceLabel":request.device_label}))
                .collect::<Vec<_>>();
            result.push(json!({"type":"devices.link.snapshot","requests":requests}));
        }
        result.extend(offered);
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
                return Err(Error::Text(text!(
                    "No device of this account can approve this device now. Start fresh on this device."
                )));
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
        credentials.enrolled = false;
        self.inner
            .state
            .lock()
            .await
            .friend_lists
            .forget(&credentials.user_id)?;
        self.forget_relist()?;
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(credentials.clone());
        Ok(credentials)
    }

    pub(super) async fn reset_devices(&self) -> Result<Vec<Value>> {
        let mut credentials = self.credentials()?;
        if credentials.enrolled {
            return Err(Error::Text(text!(
                "This device is already trusted. Remove other devices from Settings instead."
            )));
        }
        let reset = match self.request_reset(&credentials).await {
            Err(Error::Backend { status: 409, .. }) => {
                credentials = self.replace_local_keys(credentials).await?;
                self.request_reset(&credentials).await?
            }
            result => result?,
        };
        if reset.is_some_and(|reset| !reset.ready) {
            return self.device_events().await;
        }
        let pins = {
            let mut state = self.inner.state.lock().await;
            state.link = None;
            Arc::clone(&state.pins)
        };
        pins.lock()
            .map_err(|_| Error::Closed)?
            .forget(&credentials.user_id)?;
        self.enroll_fresh(&credentials).await?;
        self.ensure_enrolled().await?;
        if !self.identity().is_some_and(|identity| identity.enrolled) {
            return Err(Error::Text(text!(
                "Kodosi could not trust this device after the reset."
            )));
        }
        if let Err(error) = self.relist_friends(&self.credentials()?).await {
            tracing::warn!(%error, "the friends were not listed again after starting fresh");
        }
        let mut events = vec![self.ready_event(&credentials.user_id, true)];
        events.extend(self.device_events().await?);
        events.push(self.session_event().await?);
        Ok(events)
    }

    async fn request_reset(&self, credentials: &Credentials) -> Result<Option<PendingReset>> {
        self.inner
            .http
            .bearer(
                Method::POST,
                "api/me/identity/reset",
                &credentials.token,
                Some(json!({"deviceId":credentials.keys.device_id,"deviceLabel":host_label(),"signingPublicKey":BASE64.encode(credentials.keys.signing_public())})),
            )
            .await
    }

    async fn pending_reset(&self, credentials: &Credentials) -> Result<Option<PendingReset>> {
        match self
            .inner
            .http
            .bearer(
                Method::GET,
                "api/me/identity/reset",
                &credentials.token,
                None,
            )
            .await
        {
            Ok(reset) => Ok(Some(reset)),
            Err(Error::Backend { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn cancel_reset(&self, reset: &str) -> Result<()> {
        let credentials = self.credentials()?;
        let _response: Value = self
            .inner
            .http
            .bearer(
                Method::DELETE,
                &format!("api/me/identity/reset/{}", path_segment(reset)?),
                &credentials.token,
                None,
            )
            .await?;
        Ok(())
    }

    pub(super) async fn start_link(&self, scanned: Option<&str>) -> Result<Option<ShownCode>> {
        let credentials = self.reapproval_credentials().await?;
        if credentials.enrolled {
            return Err(Error::Text(text!("This device is already approved.")));
        }
        let secret = match scanned.map(scanned_link) {
            None => LinkSecret::Typed(new_code()?),
            Some(Some(ScannedLink {
                user_id,
                secret: secret @ LinkSecret::Scanned(_),
            })) => {
                if user_id != credentials.user_id {
                    return Err(another_account());
                }
                secret
            }
            Some(_) => {
                return Err(Error::Text(text!(
                    "Scan the code that Add a device shows on a device that you already use."
                )));
            }
        };
        let shown = match &secret {
            LinkSecret::Typed(code) => Some(ShownCode {
                code: code.clone(),
                link: secret.link(&credentials.user_id),
            }),
            LinkSecret::Scanned(_) => None,
        };
        let label = host_label();
        let mut nonce = [0; NONCE_LEN];
        aws_lc_rs::rand::fill(&mut nonce).map_err(|_| Error::Closed)?;
        let (user, keys, name) = (
            credentials.user_id.clone(),
            Arc::clone(&credentials.keys),
            label.clone(),
        );
        let (key, proof) = tokio::task::spawn_blocking(move || {
            let key = LinkKey::derive(&secret, &nonce)?;
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
        self.inner.state.lock().await.link = Some(PendingLink {
            device_code: wire::text(&link, "deviceCode")?.to_owned(),
            request_id: wire::text(&link, "requestId")?.to_owned(),
            key: Arc::new(key),
            started: Instant::now(),
            accepted: false,
        });
        Ok(shown)
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

    pub(super) async fn watch_link(&self, scanned: bool) {
        let Some(watched) = self.inner.state.lock().await.link.clone() else {
            return;
        };
        // The computer that showed a scanned code approves by itself, in less than a second.
        let pace = if scanned { LINK_POLL / 2 } else { LINK_POLL };
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = this.inner.shutdown.cancelled() => break,
                    () = tokio::time::sleep(pace) => {}
                }
                let _operation = this.inner.operations.lock().await;
                let current = this.inner.state.lock().await.link.clone();
                if current.is_none_or(|link| link.device_code != watched.device_code) {
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

    pub(super) async fn poll_link(&self) -> Result<()> {
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
        if !link.accepted {
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
                "pending" => return Ok(()),
                "approved" => {
                    let accepted = self.accept_approval(&credentials, &link, &result).await;
                    if matches!(accepted, Err(ref error) if error.unanswered()) {
                        return accepted;
                    }
                    let mut state = self.inner.state.lock().await;
                    if let Err(error) = accepted {
                        state.link = None;
                        drop(state);
                        resolved("cancelled");
                        return Err(error);
                    }
                    if let Some(current) = state
                        .link
                        .as_mut()
                        .filter(|current| current.request_id == link.request_id)
                    {
                        current.accepted = true;
                    }
                }
                state @ ("expired" | "cancelled") => {
                    self.inner.state.lock().await.link = None;
                    resolved(state);
                    return Ok(());
                }
                _ => return Err(invalid("Unsupported device approval result.")),
            }
        }
        match self.ensure_enrolled().await {
            Err(error) if error.unanswered() => return Err(error),
            settled => {
                self.inner.state.lock().await.link = None;
                let current = settled.and_then(|()| {
                    if self.identity().is_some_and(|identity| identity.enrolled) {
                        Ok(())
                    } else {
                        Err(invalid("Device approval has not become current."))
                    }
                });
                if let Err(error) = current {
                    resolved("cancelled");
                    return Err(error);
                }
            }
        }
        resolved("approved");
        self.emit_for(
            credentials.generation,
            Some(credentials.user_id.clone()),
            self.ready_event(&credentials.user_id, true),
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
            return Err(Error::Trust(text!(
                "The approval did not come from a device that has your code. Request approval again."
            )));
        }
        let verified = self.verify_bundle(&bundle, Some(root)).await?;
        if !device_identity::device_enrolled(&verified, credentials) {
            return Err(untrusted("Approval contained another device's keys."));
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

    async fn linked_device(&self, credentials: &Credentials, shown: &str) -> Result<LinkedDevice> {
        let secret = match (typed_code(shown), scanned_link(shown)) {
            (Some(code), _) => LinkSecret::Typed(code),
            (None, Some(link)) if matches!(link.secret, LinkSecret::Scanned(_)) => {
                return Err(Error::Text(text!(
                    "This code adds a new device. Scan it with the device that you want to add."
                )));
            }
            (None, Some(link)) if link.user_id != credentials.user_id => {
                return Err(another_account());
            }
            (None, Some(link)) => link.secret,
            (None, None) => {
                return Err(Error::Text(text!(
                    "Type the 12 characters of the code that the new device shows."
                )));
            }
        };
        let requests = self.link_requests(credentials).await?;
        let user = credentials.user_id.clone();
        tokio::task::spawn_blocking(move || request_with_secret(&user, &requests, &secret))
            .await
            .map_err(|_| Error::Closed)??
            .ok_or_else(|| {
                Error::Trust(text!("No device that waits for approval has this code. Compare it with the code on the new device."))
            })
    }

    pub(super) async fn link_label(&self, shown: &str) -> Result<String> {
        let credentials = self.credentials()?;
        Ok(self.linked_device(&credentials, shown).await?.label)
    }

    pub(super) async fn approve_link(&self, shown: &str) -> Result<String> {
        let credentials = self.credentials()?;
        let pending = self.linked_device(&credentials, shown).await?;
        self.approve(&credentials, pending).await
    }

    pub(super) async fn offer_link(&self) -> Result<String> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Err(Error::EnrollmentRequired);
        }
        let secret = {
            let mut state = self.inner.state.lock().await;
            if let Some(shown) = &state.offer {
                return Ok(shown.secret.link(&credentials.user_id));
            }
            let secret = Arc::new(LinkSecret::scanned()?);
            state.offer = Some(PendingOffer {
                secret: Arc::clone(&secret),
                started: tokio::time::Instant::now(),
                refused: BTreeSet::new(),
            });
            secret
        };
        let link = secret.link(&credentials.user_id);
        self.watch_offer(secret);
        Ok(link)
    }

    pub(super) async fn cancel_offer(&self) {
        self.inner.state.lock().await.offer = None;
    }

    async fn end_offer(&self, offer: &PendingOffer) {
        let mut state = self.inner.state.lock().await;
        if state
            .offer
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.secret, &offer.secret))
        {
            state.offer = None;
        }
    }

    fn watch_offer(&self, watched: Arc<LinkSecret>) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = this.inner.shutdown.cancelled() => break,
                    () = tokio::time::sleep(LINK_POLL) => {}
                }
                let _operation = this.inner.operations.lock().await;
                let current = this.inner.state.lock().await.offer.clone();
                let Some(current) = current.filter(|offer| Arc::ptr_eq(&offer.secret, &watched))
                else {
                    break;
                };
                if let Err(error) = this.poll_offer(&current).await
                    && error.user_facing()
                {
                    tracing::warn!(%error, "the devices that ask for approval were not read; they will be read again");
                }
            }
        });
    }

    pub(super) async fn poll_offer(&self, offer: &PendingOffer) -> Result<()> {
        let credentials = self.credentials()?;
        let events = if offer.started.elapsed() >= LINK_LIFE {
            self.end_offer(offer).await;
            vec![json!({"type":"devices.link.offerResolved","outcome":"expired"})]
        } else if self
            .link_requests(&credentials)
            .await?
            .iter()
            .all(|request| offer.refused.contains(&request.proof))
        {
            Vec::new()
        } else {
            self.device_events().await?
        };
        for event in events {
            self.emit_for(
                credentials.generation,
                Some(credentials.user_id.clone()),
                event,
            );
        }
        Ok(())
    }

    async fn settle_offer(&self, credentials: &Credentials) -> Vec<Value> {
        let Some(offer) = self.inner.state.lock().await.offer.clone() else {
            return Vec::new();
        };
        let asked = self.link_requests(credentials).await.and_then(|requests| {
            let unseen = requests
                .iter()
                .filter(|request| !offer.refused.contains(&request.proof));
            let pending = request_with_secret(&credentials.user_id, unseen, &offer.secret)?;
            Ok((requests, pending))
        });
        let pending = match asked {
            Ok((_, Some(pending))) => pending,
            Ok((requests, None)) => {
                if let Some(current) = self
                    .inner
                    .state
                    .lock()
                    .await
                    .offer
                    .as_mut()
                    .filter(|current| Arc::ptr_eq(&current.secret, &offer.secret))
                {
                    current.refused = requests.into_iter().map(|request| request.proof).collect();
                }
                return Vec::new();
            }
            Err(error) => {
                tracing::warn!(%error, "the devices that ask for approval were not read for the shown code");
                return Vec::new();
            }
        };
        match self.approve(credentials, pending).await {
            Ok(label) => {
                self.end_offer(&offer).await;
                vec![
                    json!({"type":"devices.link.offerResolved","outcome":"approved","deviceLabel":label}),
                ]
            }
            Err(error) if error.unanswered() || !error.user_facing() => Vec::new(),
            Err(error) => {
                self.end_offer(&offer).await;
                vec![
                    json!({"type":"devices.link.offerResolved","outcome":"cancelled","message":error.text()}),
                ]
            }
        }
    }

    async fn approve(&self, credentials: &Credentials, pending: LinkedDevice) -> Result<String> {
        let verified = self.fetch_identity(&credentials.user_id).await?;
        let new_device = pending.device_id.as_str();
        let approval =
            pending
                .key
                .approval_proof(&credentials.user_id, new_device, &verified.root)?;
        if verified.devices.contains_key(new_device) {
            return Err(Error::Text(text!("This device is already approved.")));
        }
        let mut body = device_identity::sign_addition(
            &verified,
            &device_identity::NewDevice {
                device_id: new_device,
                label: &pending.label,
                signing_public: &pending.signing_public_key,
            },
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
        )?;
        body["requestId"] = json!(pending.request_id);
        body["approvalProof"] = json!(BASE64.encode(approval));
        let _response: Value = self
            .inner
            .http
            .device(
                Method::POST,
                "api/devices/link/approve",
                credentials,
                Some(body),
            )
            .await?;
        self.fetch_identity(&credentials.user_id).await?;
        #[cfg(feature = "host")]
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
            return Err(Error::Text(text!(
                "Remove this device from another approved device."
            )));
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
            Some(issued + device_identity::DEVICE_LIST_LIFE_MS),
        )?;
        self.block_device(&credentials.user_id, id).await?;
        #[cfg(feature = "host")]
        self.review_access().await?;
        let _response:Value=self.inner.http.device(Method::POST,"api/me/identity/device-list",&credentials,Some(json!({"signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature)}))).await?;
        self.fetch_identity(&credentials.user_id).await?;
        #[cfg(feature = "host")]
        self.review_access().await?;
        Ok(())
    }
}
