use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use serde::{Deserialize, Serialize};

use super::{BASE64, IdentityBundleView};
use crate::{
    AppError, Result,
    identity_core::{
        identity_bundle_view::{IdentityBundleInput, IdentityDeviceInput, IdentityDeviceListInput},
        signed_device_list::SignedDeviceList,
    },
};

pub(super) const PIN_FILE_SCHEMA_VERSION: u32 = 4;

const fn legacy_identity_revision() -> u64 {
    1
}

pub(super) const fn legacy_identity_incarnation() -> uuid::Uuid {
    uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_00ff)
}

pub(super) fn is_legacy_identity_incarnation(incarnation_id: uuid::Uuid) -> bool {
    incarnation_id.is_nil() || incarnation_id == legacy_identity_incarnation()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeviceListPin {
    pub(crate) user_id: String,
    #[serde(default = "legacy_identity_revision")]
    pub(crate) identity_revision: u64,
    #[serde(default = "legacy_identity_incarnation")]
    pub(crate) identity_incarnation_id: uuid::Uuid,
    pub(crate) list_body_b64: String,
    pub(crate) list_signature_b64: String,
    #[serde(default)]
    pub(crate) device_proofs: BTreeMap<String, PinnedDeviceProof>,
    pub(crate) pinned_at_ms: i64,
    #[serde(default)]
    pub(crate) revoked_device_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinnedProofSummary {
    pub(crate) generation: u64,
    pub(crate) signer_device_id: String,
    pub(crate) active_device_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PinnedDeviceProof {
    pub(crate) certificate: String,
    pub(crate) certificate_signature: String,
}

impl DeviceListPin {
    pub(super) fn from_view(
        view: &IdentityBundleView,
        pinned_at_ms: i64,
        revoked_device_ids: BTreeSet<String>,
    ) -> Self {
        let device_proofs = view
            .devices
            .iter()
            .chain(&view.historical_devices)
            .map(|(device_id, device)| {
                (
                    device_id.clone(),
                    PinnedDeviceProof {
                        certificate: BASE64.encode(&device.certificate_body),
                        certificate_signature: BASE64.encode(&device.certificate_signature),
                    },
                )
            })
            .collect();
        Self {
            user_id: view.user_id.clone(),
            identity_revision: view.identity_revision,
            identity_incarnation_id: view.identity_incarnation_id,
            list_body_b64: BASE64.encode(&view.list_body),
            list_signature_b64: BASE64.encode(&view.list_signature),
            device_proofs,
            pinned_at_ms,
            revoked_device_ids,
        }
    }

    pub(crate) fn proof_summary(&self) -> Result<PinnedProofSummary> {
        let body =
            BASE64
                .decode(&self.list_body_b64)
                .map_err(|error| AppError::InvalidBackendData {
                    field: "pin.list_body_b64".to_owned(),
                    reason: format!("base64 decode failed: {error}"),
                })?;
        let list = SignedDeviceList::parse_body(&body)?;
        Ok(PinnedProofSummary {
            generation: list.generation,
            signer_device_id: list.signer_device_id,
            active_device_ids: list
                .entries
                .into_iter()
                .map(|entry| entry.device_id)
                .collect(),
        })
    }

    pub(crate) fn identity_bundle_input(&self) -> Result<Option<IdentityBundleInput>> {
        let body = BASE64
            .decode(&self.list_body_b64)
            .map_err(|error| AppError::Unsupported {
                reason: format!("corrupt pinned device-list body: {error}"),
            })?;
        let list = SignedDeviceList::parse_body(&body)?;
        let mut devices = Vec::with_capacity(list.entries.len());
        for entry in &list.entries {
            let Some(proof) = self.device_proofs.get(&entry.device_id) else {
                return Ok(None);
            };
            devices.push(identity_device_input(&entry.device_id, proof)?);
        }
        let historical_devices: Vec<IdentityDeviceInput> = self
            .device_proofs
            .iter()
            .filter(|(device_id, _)| {
                !list
                    .entries
                    .iter()
                    .any(|entry| &entry.device_id == *device_id)
            })
            .map(|(device_id, proof)| identity_device_input(device_id, proof))
            .collect::<Result<_>>()?;
        Ok(Some(IdentityBundleInput {
            user_id: self.user_id.clone(),
            device_list: IdentityDeviceListInput {
                generation: list.generation,
                signer_device_id: list.signer_device_id,
                issued_at_ms: list.issued_at_ms,
                expires_at_ms: list.expires_at_ms,
                body: self.list_body_b64.clone(),
                signature: self.list_signature_b64.clone(),
            },
            devices,
            historical_devices,
        }))
    }
}

fn identity_device_input(
    device_id: &str,
    proof: &PinnedDeviceProof,
) -> Result<IdentityDeviceInput> {
    let body = BASE64
        .decode(&proof.certificate)
        .map_err(|error| AppError::InvalidBackendData {
            field: "pin.device_proofs.certificate".to_owned(),
            reason: format!("base64 decode failed: {error}"),
        })?;
    let certificate = crate::identity_core::device_cert::DeviceCertificate::parse_body(&body)?;
    if certificate.device_id != device_id {
        return Err(AppError::InvalidBackendData {
            field: "pin.device_proofs".to_owned(),
            reason: format!(
                "certificate device ID {} does not match proof key {device_id}",
                certificate.device_id
            ),
        });
    }
    Ok(IdentityDeviceInput {
        device_id: device_id.to_owned(),
        kem_public_key: BASE64.encode(&certificate.kem_public_key),
        signing_public_key: BASE64.encode(&certificate.sig_public_key),
        certificate: proof.certificate.clone(),
        certificate_signature: proof.certificate_signature.clone(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityLifecycleWatermark {
    pub(crate) revision: u64,
    pub(crate) incarnation_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct AccountPinBucket {
    #[serde(default)]
    pub(super) pins: BTreeMap<String, DeviceListPin>,
    #[serde(default)]
    pub(super) identity_lifecycles: BTreeMap<String, IdentityLifecycleWatermark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PinFile {
    pub(super) version: u32,
    #[serde(default)]
    pub(super) revision: u64,

    #[serde(default)]
    pub(super) last_bound_owner_user_id: Option<String>,
    #[serde(default)]
    pub(super) accounts: BTreeMap<String, AccountPinBucket>,
}

impl PinFile {
    pub(super) fn pins_for(
        &self,
        owner_user_id: Option<&str>,
    ) -> Option<&BTreeMap<String, DeviceListPin>> {
        owner_user_id.and_then(|owner| self.accounts.get(owner).map(|bucket| &bucket.pins))
    }

    pub(super) fn pins_for_mut(
        &mut self,
        owner_user_id: &str,
    ) -> &mut BTreeMap<String, DeviceListPin> {
        &mut self
            .accounts
            .entry(owner_user_id.to_owned())
            .or_default()
            .pins
    }
}

impl Default for PinFile {
    fn default() -> Self {
        Self {
            version: PIN_FILE_SCHEMA_VERSION,
            revision: 0,
            last_bound_owner_user_id: None,
            accounts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LegacyAccountPinBucketV2 {
    #[serde(default)]
    pub(super) pins: BTreeMap<String, DeviceListPin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LegacyPinFileV2 {
    pub(super) version: u32,
    #[serde(default)]
    pub(super) revision: u64,
    #[serde(default)]
    pub(super) last_bound_owner_user_id: Option<String>,
    #[serde(default)]
    pub(super) accounts: BTreeMap<String, LegacyAccountPinBucketV2>,
}

impl From<LegacyPinFileV2> for PinFile {
    fn from(legacy: LegacyPinFileV2) -> Self {
        Self {
            version: PIN_FILE_SCHEMA_VERSION,
            revision: legacy.revision,
            last_bound_owner_user_id: legacy.last_bound_owner_user_id,
            accounts: legacy
                .accounts
                .into_iter()
                .map(|(owner, bucket)| {
                    (
                        owner,
                        AccountPinBucket {
                            pins: bucket.pins,
                            identity_lifecycles: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct LegacyPinFileV1 {
    #[serde(rename = "version")]
    pub(super) _version: u32,
    #[serde(default)]
    pub(super) revision: u64,
    #[serde(default)]
    pub(super) owner_user_id: Option<String>,
    #[serde(default)]
    pub(super) pins: BTreeMap<String, DeviceListPin>,
}

impl From<LegacyPinFileV1> for PinFile {
    fn from(legacy: LegacyPinFileV1) -> Self {
        let mut accounts = BTreeMap::new();
        if let Some(owner) = legacy.owner_user_id.as_ref() {
            accounts.insert(
                owner.clone(),
                AccountPinBucket {
                    pins: legacy.pins,
                    identity_lifecycles: BTreeMap::new(),
                },
            );
        }
        Self {
            version: PIN_FILE_SCHEMA_VERSION,
            revision: legacy.revision,
            last_bound_owner_user_id: legacy.owner_user_id,
            accounts,
        }
    }
}
