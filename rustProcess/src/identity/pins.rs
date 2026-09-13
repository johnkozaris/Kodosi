use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    device_cert::{DeviceCertificate, verify_certificate},
    signed_device_list::{SignedDeviceList, verify_signed_device_list},
    storage::private_write,
};
use crate::network::{Error, Result, invalid};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CertificateEnvelope {
    pub certificate: String,
    pub certificate_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceListEnvelope {
    pub body: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityBundle {
    pub user_id: String,
    pub identity_revision: u64,
    pub identity_incarnation_id: Uuid,
    pub device_list: DeviceListEnvelope,
    pub devices: Vec<CertificateEnvelope>,
    #[serde(default)]
    pub certificate_chain: Vec<CertificateEnvelope>,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedIdentity {
    pub generation: u64,
    pub devices: BTreeMap<String, DeviceCertificate>,
    pub list: SignedDeviceList,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Pin {
    identity_incarnation_id: Uuid,
    identity_revision: u64,
    generation: u64,
    list_body: String,
    list_signature: String,
    certificates: BTreeMap<String, CertificateEnvelope>,
    revoked: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredPins {
    version: u8,
    pins: BTreeMap<String, Pin>,
}

pub struct Pins {
    path: PathBuf,
    pins: BTreeMap<String, Pin>,
}

impl Pins {
    pub fn load(path: PathBuf) -> Result<Self> {
        let pins = match std::fs::read(&path) {
            Ok(bytes) => {
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err(invalid("Saved device trust exceeds its limit."));
                }
                let stored: StoredPins = serde_json::from_slice(&bytes)?;
                if stored.version != 1 {
                    return Err(invalid("Saved device trust uses an unsupported format."));
                }
                stored.pins
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { path, pins })
    }

    pub(crate) fn is_revoked(&self, user_id: &str, device_id: &str) -> bool {
        self.pins
            .get(user_id)
            .is_some_and(|pin| pin.revoked.contains(device_id))
    }

    pub(crate) fn contains(&self, user_id: &str) -> bool {
        self.pins.contains_key(user_id)
    }

    pub(crate) fn verify(
        &mut self,
        bundle: &IdentityBundle,
        allow_first: bool,
        now_ms: u64,
    ) -> Result<VerifiedIdentity> {
        self.verify_current(bundle, allow_first, now_ms, false)
    }

    pub(crate) fn verify_for_renewal(
        &mut self,
        bundle: &IdentityBundle,
        now_ms: u64,
    ) -> Result<VerifiedIdentity> {
        self.verify_current(bundle, false, now_ms, true)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one linear validate-then-persist trust transaction; no partial pin updates"
    )]
    fn verify_current(
        &mut self,
        bundle: &IdentityBundle,
        allow_first: bool,
        now_ms: u64,
        allow_expired: bool,
    ) -> Result<VerifiedIdentity> {
        if bundle.identity_incarnation_id.is_nil()
            || bundle.identity_revision == 0
            || bundle.devices.is_empty()
            || bundle.devices.len() > 256
        {
            return Err(invalid("Identity bundle is incomplete."));
        }
        let list_body = decode(&bundle.device_list.body, 527_432)?;
        let list_signature = decode(&bundle.device_list.signature, 3309)?;
        let list = SignedDeviceList::parse_body(&list_body)?;
        if list.user_id != bundle.user_id {
            return Err(invalid("Device list belongs to another account."));
        }
        let existing = self.pins.get(&bundle.user_id);
        if existing.is_none() && !allow_first {
            return Err(Error::Trust(
                "This person has not been explicitly trusted.".into(),
            ));
        }
        if let Some(pin) = existing {
            if pin.identity_incarnation_id != bundle.identity_incarnation_id
                || bundle.identity_revision < pin.identity_revision
                || list.generation < pin.generation
            {
                return Err(Error::Trust("This account's device identity changed. Approve the replacement before reconnecting.".into()));
            }
            if list.generation > pin.generation {
                let previous = SignedDeviceList::parse_body(&decode(&pin.list_body, 527_432)?)?;
                if list.issued_at_ms <= previous.issued_at_ms {
                    return Err(Error::Trust("Device list issuance did not advance.".into()));
                }
            }
            if list.generation == pin.generation
                && (bundle.device_list.body != pin.list_body
                    || bundle.device_list.signature != pin.list_signature)
            {
                return Err(Error::Trust(
                    "A device list changed without a newer generation.".into(),
                ));
            }
        }
        if list.issued_at_ms > now_ms.saturating_add(5 * 60_000)
            || (!allow_expired && list.expires_at_ms.is_some_and(|expiry| expiry <= now_ms))
        {
            return Err(Error::Trust(
                "The signed device list is no longer current.".into(),
            ));
        }

        let mut incoming = BTreeMap::new();
        let mut bodies = BTreeMap::new();
        for envelope in &bundle.devices {
            let body = decode(&envelope.certificate, 5772)?;
            let cert = DeviceCertificate::parse_body(&body)?;
            if cert.user_id != bundle.user_id
                || incoming
                    .insert(cert.device_id.clone(), envelope.clone())
                    .is_some()
            {
                return Err(invalid(
                    "Identity bundle contains a foreign or duplicate device.",
                ));
            }
            if cert.issued_at_ms > now_ms.saturating_add(5 * 60_000) || !cert.is_valid_at(now_ms) {
                return Err(Error::Trust(
                    "A device certificate is expired or not yet valid.".into(),
                ));
            }
            bodies.insert(cert.device_id.clone(), (cert, body));
        }
        let entries = list
            .entries
            .iter()
            .map(|entry| entry.device_id.clone())
            .collect::<BTreeSet<_>>();
        if entries != incoming.keys().cloned().collect() {
            return Err(invalid("Device list and certificates disagree."));
        }
        let mut certificates = incoming.clone();
        if bundle.certificate_chain.len() > 256 {
            return Err(invalid("Device certificate ancestry exceeds its bound."));
        }
        for envelope in &bundle.certificate_chain {
            let body = decode(&envelope.certificate, 5772)?;
            let certificate = DeviceCertificate::parse_body(&body)?;
            if certificate.user_id != bundle.user_id || bodies.contains_key(&certificate.device_id)
            {
                return Err(invalid(
                    "Certificate ancestry contains a foreign or duplicate device.",
                ));
            }
            certificates.insert(certificate.device_id.clone(), envelope.clone());
            bodies.insert(certificate.device_id.clone(), (certificate, body));
        }
        let mut prior = BTreeMap::new();
        if let Some(pin) = existing {
            for (id, envelope) in &pin.certificates {
                prior.insert(
                    id.clone(),
                    DeviceCertificate::parse_body(&decode(&envelope.certificate, 5772)?)?,
                );
            }
        }
        let list_signer = bodies
            .get(&list.signer_device_id)
            .ok_or_else(|| invalid("The device-list signer is not active."))?;
        if let Some(pin) = existing {
            if pin.revoked.contains(&list.signer_device_id)
                || entries.iter().any(|id| pin.revoked.contains(id))
            {
                return Err(Error::Trust(
                    "A removed device cannot become trusted again.".into(),
                ));
            }
            if list.generation > pin.generation {
                let mut signer = list.signer_device_id.as_str();
                let mut seen = BTreeSet::new();
                loop {
                    if !seen.insert(signer) || pin.revoked.contains(signer) {
                        return Err(Error::Trust(
                            "Device update has no currently trusted signer.".into(),
                        ));
                    }
                    let candidate = &bodies
                        .get(signer)
                        .ok_or_else(|| invalid("Device signer is absent."))?
                        .0;
                    if let Some(trusted) = prior.get(signer) {
                        if trusted != candidate {
                            return Err(Error::Trust(
                                "A previously trusted signer changed its certificate.".into(),
                            ));
                        }
                        break;
                    }
                    if candidate.is_self_signed() {
                        return Err(Error::Trust(
                            "Device update is not endorsed by an already trusted device.".into(),
                        ));
                    }
                    signer = &candidate.signer_device_id;
                }
            }
            for (id, envelope) in &incoming {
                if let Some(previous) = pin.certificates.get(id)
                    && (previous.certificate != envelope.certificate
                        || previous.certificate_signature != envelope.certificate_signature)
                {
                    return Err(Error::Trust(
                        "An existing device changed its key material.".into(),
                    ));
                }
            }
        }
        verify_signed_device_list(&list_body, &list_signature, &list_signer.0.sig_public_key)?;
        for (device_id, (cert, body)) in &bodies {
            if entries.contains(device_id)
                && !list.entries.iter().any(|entry| {
                    entry.device_id == *device_id && entry.signer_device_id == cert.signer_device_id
                })
            {
                return Err(invalid("Certificate signer and device list disagree."));
            }
            let signer = bodies
                .get(&cert.signer_device_id)
                .map(|x| &x.0)
                .or_else(|| prior.get(&cert.signer_device_id))
                .ok_or_else(|| {
                    Error::Trust("A device certificate has no verifiable signer.".into())
                })?;
            if signer.issued_at_ms > cert.issued_at_ms || !signer.is_valid_at(cert.issued_at_ms) {
                return Err(Error::Trust(
                    "Certificate was issued outside its signer's validity.".into(),
                ));
            }
            verify_certificate(
                body,
                &decode(&certificates[device_id].certificate_signature, 3309)?,
                &signer.sig_public_key,
            )?;
            validate_bootstrap_chain(device_id, &bodies)?;
        }
        let mut revoked = existing.map(|p| p.revoked.clone()).unwrap_or_default();
        revoked.extend(bodies.keys().filter(|id| !entries.contains(*id)).cloned());
        if let Some(pin) = existing {
            revoked.extend(
                pin.certificates
                    .keys()
                    .filter(|id| !entries.contains(*id))
                    .cloned(),
            );
        }
        let pin = Pin {
            identity_incarnation_id: bundle.identity_incarnation_id,
            identity_revision: bundle.identity_revision,
            generation: list.generation,
            list_body: bundle.device_list.body.clone(),
            list_signature: bundle.device_list.signature.clone(),
            certificates: incoming,
            revoked,
        };
        if self.pins.get(&bundle.user_id) != Some(&pin) {
            let mut next = self.pins.clone();
            next.insert(bundle.user_id.clone(), pin);
            private_write(
                &self.path,
                &serde_json::to_vec(&StoredPins {
                    version: 1,
                    pins: next.clone(),
                })?,
            )?;
            self.pins = next;
        }
        Ok(VerifiedIdentity {
            generation: list.generation,
            devices: bodies
                .into_iter()
                .filter(|(id, _)| entries.contains(id))
                .map(|(id, (cert, _))| (id, cert))
                .collect(),
            list,
        })
    }
}

fn validate_bootstrap_chain(
    start: &str,
    bodies: &BTreeMap<String, (DeviceCertificate, Vec<u8>)>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut current = start;
    loop {
        if !seen.insert(current) {
            return Err(invalid("Device certificate signing cycle."));
        }
        let cert = &bodies
            .get(current)
            .ok_or_else(|| invalid("Device signer is absent."))?
            .0;
        if cert.is_self_signed() {
            return Ok(());
        }
        current = &cert.signer_device_id;
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn decode(value: &str, limit: usize) -> Result<Vec<u8>> {
    if value.len() > limit.saturating_add(2) / 3 * 4 {
        return Err(invalid("Encoded identity field exceeds its limit."));
    }
    let bytes = BASE64
        .decode(value)
        .map_err(|_| invalid("Identity field is not base64."))?;
    if bytes.len() > limit {
        return Err(invalid("Decoded identity field exceeds its limit."));
    }
    Ok(bytes)
}
