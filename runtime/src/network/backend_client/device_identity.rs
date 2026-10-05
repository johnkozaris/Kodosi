use super::*;
use crate::identity::pins::{Anchor, Root, root_of};

impl BackendClient {
    pub(super) async fn pinned(&self, user_id: &str) -> Result<bool> {
        Ok(self
            .inner
            .state
            .lock()
            .await
            .pins
            .lock()
            .map_err(|_| Error::Closed)?
            .contains(user_id))
    }

    pub(super) async fn bootstrap_identity(
        &self,
        credentials: &Credentials,
    ) -> Result<(IdentityBundle, Root)> {
        if self.pinned(&credentials.user_id).await? {
            return Err(Error::Trust(
                "The server has no trusted devices for this account, but this device knows some."
                    .into(),
            ));
        }
        let now = identity::now_ms();
        let cert = build_self_cert(
            &credentials.user_id,
            &credentials.keys.device_id,
            &host_label(),
            &credentials.keys.signing_key()?,
            now,
            None,
        )?;
        let root = root_of(&cert.body_bytes);
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
        "deviceId":credentials.keys.device_id,"signingPublicKey":BASE64.encode(credentials.keys.signing_public()),
        "challengeId":challenge["challengeId"],"popSignature":BASE64.encode(signature),"deviceCertificate":BASE64.encode(cert.body_bytes),"deviceCertificateSignature":BASE64.encode(cert.signature),
        "signedDeviceList":BASE64.encode(list.body_bytes),"signedDeviceListSignature":BASE64.encode(list.signature),
    }))).await?;
        let bundle = self
            .inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await?;
        Ok((bundle, root))
    }

    pub(super) async fn ensure_enrolled(&self) -> Result<()> {
        if self.settle_enrollment().await? {
            self.start_notifications().await?;
        } else {
            let notifications = self.inner.notifications.lock().await.take();
            if let Some(cancel) = notifications {
                cancel.cancel();
            }
        }
        self.inner.identity_settled.store(true, Ordering::Release);
        Ok(())
    }

    pub(super) async fn settle_enrollment(&self) -> Result<bool> {
        let started = self.credentials()?;
        let (verified, notice) = match self.own_standing(started.clone()).await {
            Ok((verified, true)) => (verified, REPLACED.to_owned()),
            Ok((verified, false)) => (verified, UNAPPROVED.to_owned()),
            Err(Error::Invalid { reason }) => {
                tracing::warn!(%reason, "the trusted devices of this account are not readable");
                (None, UNREADABLE.to_owned())
            }
            Err(Error::Trust(reason)) => {
                tracing::warn!(%reason, "the trusted devices of this account are not confirmed");
                (
                    None,
                    format!(
                        "{reason} Approve this device from another device, or start fresh here."
                    ),
                )
            }
            Err(error) => return Err(error),
        };
        self.check_credentials(&started)?;
        let mut credentials = self.credentials()?;
        let enrolled = verified
            .as_ref()
            .is_some_and(|verified| device_enrolled(verified, &credentials));
        if credentials.enrolled && !enrolled {
            self.suspend_transports().await;
        }
        credentials.enrolled = enrolled;
        credentials.notice = (!enrolled).then_some(notice);
        *self.inner.credentials.write().map_err(|_| Error::Closed)? = Some(credentials);
        if let Some(identity) = self
            .inner
            .identity
            .write()
            .map_err(|_| Error::Closed)?
            .as_mut()
        {
            identity.enrolled = enrolled;
        }
        Ok(enrolled)
    }

    async fn own_standing(
        &self,
        credentials: Credentials,
    ) -> Result<(Option<VerifiedIdentity>, bool)> {
        let result = self
            .inner
            .http
            .bearer::<IdentityBundle>(Method::GET, "api/me/identity", &credentials.token, None)
            .await;
        let (bundle, created) = match result {
            Ok(bundle) => (bundle, None),
            Err(Error::Backend { status: 404, .. }) => {
                let (bundle, root) = self.bootstrap_identity(&credentials).await?;
                (bundle, Some(root))
            }
            Err(error) => return Err(error),
        };
        self.check_credentials(&credentials)?;
        if bundle.user_id != credentials.user_id {
            return Err(Error::Trust(
                "The server sent the trusted devices of another account.".into(),
            ));
        }
        let now = identity::now_ms();
        let pins = Arc::clone(&self.inner.state.lock().await.pins);
        let (credentials, replaced) = self
            .adopt_replaced_identity(credentials, &bundle, &pins)
            .await?;
        let mut verified = own_identity(pins, bundle, created, now).await?;
        if let Some(current) = verified.as_ref().filter(|current| {
            device_enrolled(current, &credentials)
                && current
                    .list
                    .expires_at_ms
                    .is_some_and(|expiry| expiry <= now + 6 * 60 * 60_000)
        }) {
            self.renew_device_list(&credentials, current, now).await?;
            verified = Some(self.fetch_identity(&credentials.user_id).await?);
        }
        if verified.as_ref().is_some_and(|verified| {
            verified
                .list
                .expires_at_ms
                .is_some_and(|expiry| expiry <= now)
        }) {
            return Err(Error::Trust(
                "The list of trusted devices needs renewal from an approved device.".into(),
            ));
        }
        Ok((verified, replaced))
    }

    async fn adopt_replaced_identity(
        &self,
        credentials: Credentials,
        bundle: &IdentityBundle,
        pins: &Arc<std::sync::Mutex<Pins>>,
    ) -> Result<(Credentials, bool)> {
        {
            let mut guard = pins.lock().map_err(|_| Error::Closed)?;
            if !guard.own_identity_replaced(bundle) {
                return Ok((credentials, false));
            }
            guard.forget(&credentials.user_id)?;
        }
        self.inner.state.lock().await.link = None;
        Ok((self.replace_local_keys(credentials).await?, true))
    }

    pub(super) async fn renew_device_list(
        &self,
        credentials: &Credentials,
        verified: &VerifiedIdentity,
        now: u64,
    ) -> Result<()> {
        let issued = verified.list.successor_issued_at(now)?;
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
        root: Option<Root>,
    ) -> Result<VerifiedIdentity> {
        let pins = Arc::clone(&self.inner.state.lock().await.pins);
        let bundle = bundle.clone();
        tokio::task::spawn_blocking(move || {
            let anchor = root.as_ref().map_or(Anchor::Pinned, Anchor::Root);
            pins.lock()
                .map_err(|_| Error::Closed)?
                .verify(&bundle, anchor, identity::now_ms())
        })
        .await
        .map_err(|_| Error::Closed)?
    }

    pub(crate) async fn fetch_identity(&self, user_id: &str) -> Result<VerifiedIdentity> {
        let credentials = self.credentials()?;
        self.fetch_identity_with(&credentials, user_id).await
    }

    pub(crate) async fn fetch_identity_with(
        &self,
        credentials: &Credentials,
        user_id: &str,
    ) -> Result<VerifiedIdentity> {
        self.check_credentials(credentials)?;
        if user_id != credentials.user_id {
            return self.friend_identity(credentials, user_id).await;
        }
        let kept = self.kept_identity(credentials, user_id);
        let fresh: Option<(IdentityBundle, String)> = self
            .inner
            .http
            .tagged(
                "api/me/identity",
                credentials,
                false,
                kept.as_ref().map(|kept| kept.tag.as_str()),
            )
            .await?;
        self.check_credentials(credentials)?;
        let verified = match (fresh, kept) {
            (None, Some(kept)) => {
                self.keep_identity(credentials, user_id, &kept.identity, kept.tag);
                kept.identity
            }
            (None, None) => return Err(invalid("The server sent no device identity.")),
            (Some((bundle, tag)), _) => {
                if bundle.user_id != user_id {
                    return Err(Error::Trust(
                        "Returned device identity belongs to another account.".into(),
                    ));
                }
                let verified = self.verify_bundle(&bundle, None).await?;
                self.keep_identity(credentials, user_id, &verified, tag);
                verified
            }
        };
        self.without_blocked_devices(user_id, verified).await
    }

    pub(crate) async fn known_identity(
        &self,
        credentials: &Credentials,
        user_id: &str,
    ) -> Result<VerifiedIdentity> {
        let kept = self
            .kept_identity(credentials, user_id)
            .filter(|kept| kept.checked.elapsed() < KEPT_IDENTITY);
        let changed = self
            .inner
            .state
            .lock()
            .await
            .changed_friends
            .contains(user_id);
        match kept {
            Some(kept) if !changed => self.without_blocked_devices(user_id, kept.identity).await,
            _ => self.fetch_identity_with(credentials, user_id).await,
        }
    }

    pub(super) fn kept_identity(
        &self,
        credentials: &Credentials,
        user_id: &str,
    ) -> Option<KeptIdentity> {
        let now = identity::now_ms();
        self.inner
            .identities
            .lock()
            .ok()?
            .get(user_id)
            .filter(|kept| {
                kept.generation == credentials.generation
                    && kept
                        .identity
                        .list
                        .expires_at_ms
                        .is_none_or(|expiry| now < expiry)
                    && kept
                        .identity
                        .devices
                        .values()
                        .all(|certificate| certificate.is_valid_at(now))
            })
            .cloned()
    }

    pub(super) fn keep_identity(
        &self,
        credentials: &Credentials,
        user_id: &str,
        identity: &VerifiedIdentity,
        tag: String,
    ) {
        if tag.is_empty() {
            return;
        }
        if let Ok(mut kept) = self.inner.identities.lock() {
            kept.retain(|_, kept| kept.generation == credentials.generation);
            kept.insert(
                user_id.to_owned(),
                KeptIdentity {
                    generation: credentials.generation,
                    identity: identity.clone(),
                    tag,
                    checked: tokio::time::Instant::now(),
                },
            );
        }
    }

    pub(super) fn forget_identity(&self, user_id: &str) {
        if let Ok(mut kept) = self.inner.identities.lock() {
            kept.remove(user_id);
        }
    }

    pub(super) async fn without_blocked_devices(
        &self,
        user_id: &str,
        mut verified: VerifiedIdentity,
    ) -> Result<VerifiedIdentity> {
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

async fn own_identity(
    pins: Arc<std::sync::Mutex<Pins>>,
    bundle: IdentityBundle,
    created: Option<Root>,
    now: u64,
) -> Result<Option<VerifiedIdentity>> {
    tokio::task::spawn_blocking(move || {
        let mut pins = pins.lock().map_err(|_| Error::Closed)?;
        match created {
            Some(root) => pins.verify(&bundle, Anchor::Root(&root), now).map(Some),
            None if pins.contains(&bundle.user_id) => {
                pins.verify_for_renewal(&bundle, now).map(Some)
            }
            None => Ok(None),
        }
    })
    .await
    .map_err(|_| Error::Closed)?
}

const UNAPPROVED: &str = "Approve this device from one of your existing devices.";
const REPLACED: &str = "Your trusted devices were reset from another device. Approve this device from that device, or start fresh here.";
const UNREADABLE: &str = "This version of Kodosi cannot read the trusted devices of this account. Start fresh on this device, then approve your other devices again.";

pub(super) fn device_enrolled(verified: &VerifiedIdentity, credentials: &Credentials) -> bool {
    verified
        .devices
        .get(&credentials.keys.device_id)
        .is_some_and(|cert| cert.sig_public_key == credentials.keys.signing_public())
}

const KEPT_IDENTITY: Duration = Duration::from_mins(10);

#[derive(Clone)]
pub(crate) struct KeptIdentity {
    generation: u64,
    pub(super) identity: VerifiedIdentity,
    pub(super) tag: String,
    checked: tokio::time::Instant,
}
