use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use crate::{
    AppError, Result,
    identity_core::{
        device_cert::{DeviceCertificate, verify_certificate},
        signed_device_list::{SignedDeviceList, verify_signed_device_list},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ExactIdentityBundleInput {
    pub(crate) user_id: String,
    pub(crate) list_body: String,
    pub(crate) list_signature: String,
    pub(crate) devices: Vec<ExactIdentityDeviceInput>,
    pub(crate) historical_devices: Vec<ExactIdentityDeviceInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExactIdentityDeviceInput {
    pub(crate) certificate: String,
    pub(crate) certificate_signature: String,
}

pub(crate) fn build_exact_bundle_view(
    dto: &ExactIdentityBundleInput,
) -> Result<IdentityBundleView> {
    let device_list = exact_device_list_input(&dto.list_body, &dto.list_signature)?;
    let devices = dto
        .devices
        .iter()
        .map(exact_device_input)
        .collect::<Result<Vec<_>>>()?;
    let historical_devices = dto
        .historical_devices
        .iter()
        .map(exact_device_input)
        .collect::<Result<Vec<_>>>()?;
    build_bundle_view(&IdentityBundleInput {
        user_id: dto.user_id.clone(),
        device_list,
        devices,
        historical_devices,
    })
}

fn exact_device_list_input(body: &str, signature: &str) -> Result<IdentityDeviceListInput> {
    let body_bytes = decode_b64(body, "device_list.body")?;
    let list = SignedDeviceList::parse_body(&body_bytes)?;
    Ok(IdentityDeviceListInput {
        generation: list.generation,
        signer_device_id: list.signer_device_id,
        issued_at_ms: list.issued_at_ms,
        expires_at_ms: list.expires_at_ms,
        body: body.to_owned(),
        signature: signature.to_owned(),
    })
}

fn exact_device_input(device: &ExactIdentityDeviceInput) -> Result<IdentityDeviceInput> {
    let certificate_bytes = decode_b64(&device.certificate, "device.certificate")?;
    let certificate = DeviceCertificate::parse_body(&certificate_bytes)?;
    Ok(IdentityDeviceInput {
        device_id: certificate.device_id,
        kem_public_key: BASE64.encode(certificate.kem_public_key),
        signing_public_key: BASE64.encode(certificate.sig_public_key),
        certificate: device.certificate.clone(),
        certificate_signature: device.certificate_signature.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinContext {
    ExplicitShare,
    PeerEnrollment,
    BackgroundFetch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PinVerdict {
    FirstShare,
    AcceptUpdate,
    AlreadyPinned,
    Reject { reason: BreakReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BreakReason {
    NoChainToPin,
    StaleGeneration,
    NoExplicitShareContext,
    SignerRevoked,
    RevokedDeviceReappeared,
    KeyMaterialChanged,
    IdentityLifecycleChanged,
    IdentityExpired,
}

#[derive(Debug, Clone)]
pub(crate) struct IdentityBundleView {
    pub(crate) user_id: String,
    pub(crate) identity_revision: u64,
    pub(crate) identity_incarnation_id: uuid::Uuid,
    pub(crate) signed_list: SignedDeviceList,
    pub(crate) list_body: Vec<u8>,
    pub(crate) list_signature: Vec<u8>,
    pub(crate) devices: BTreeMap<String, VerifiedDevice>,
    pub(crate) historical_devices: BTreeMap<String, VerifiedDevice>,
}

impl IdentityBundleView {
    pub(crate) fn signing_device_at(
        &self,
        device_id: &str,
        signed_at_ms: u64,
    ) -> Option<&VerifiedDevice> {
        self.devices.get(device_id).filter(|device| {
            device.certificate.issued_at_ms <= signed_at_ms
                && device.certificate.is_valid_at(signed_at_ms)
        })
    }

    pub(crate) fn is_valid_at(&self, now_ms: u64) -> bool {
        const ISSUED_AT_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
        let issued_cutoff = now_ms.saturating_add(ISSUED_AT_CLOCK_SKEW_MS);
        self.signed_list.issued_at_ms <= issued_cutoff
            && self
                .signed_list
                .expires_at_ms
                .is_none_or(|expires| now_ms < expires)
            && self.devices.values().all(|device| {
                device.certificate.issued_at_ms <= issued_cutoff
                    && device.certificate.is_valid_at(now_ms)
            })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedDevice {
    pub(crate) certificate: DeviceCertificate,
    pub(crate) sig_public_key: Vec<u8>,
    pub(crate) certificate_body: Vec<u8>,
    pub(crate) certificate_signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityBundleInput {
    pub(crate) user_id: String,
    pub(crate) device_list: IdentityDeviceListInput,
    pub(crate) devices: Vec<IdentityDeviceInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) historical_devices: Vec<IdentityDeviceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityDeviceListInput {
    pub(crate) generation: u64,
    pub(crate) signer_device_id: String,
    pub(crate) issued_at_ms: u64,
    pub(crate) expires_at_ms: Option<u64>,
    pub(crate) body: String,
    pub(crate) signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityDeviceInput {
    pub(crate) device_id: String,
    pub(crate) kem_public_key: String,
    pub(crate) signing_public_key: String,
    pub(crate) certificate: String,
    pub(crate) certificate_signature: String,
}

pub(crate) struct HistoricalIdentityDeviceInput {
    pub(crate) device: IdentityDeviceInput,
}

pub(crate) fn build_bundle_view(dto: &IdentityBundleInput) -> Result<IdentityBundleView> {
    let historical = dto
        .historical_devices
        .iter()
        .cloned()
        .map(|device| HistoricalIdentityDeviceInput { device })
        .collect::<Vec<_>>();
    build_bundle_view_with_historical(dto, &historical)
}

#[expect(
    clippy::too_many_lines,
    reason = "verification is naturally sequential; splitting into helpers would scatter the trust-graph reasoning"
)]
pub(crate) fn build_bundle_view_with_historical(
    dto: &IdentityBundleInput,
    historical_devices: &[HistoricalIdentityDeviceInput],
) -> Result<IdentityBundleView> {
    let list_body = decode_b64(&dto.device_list.body, "device_list.body")?;
    let list_signature = decode_b64(&dto.device_list.signature, "device_list.signature")?;

    let mut decoded_devices: Vec<DecodedDevice> = dto
        .devices
        .iter()
        .map(decode_device_input)
        .collect::<Result<_>>()?;
    let mut decoded_historical: Vec<DecodedDevice> = historical_devices
        .iter()
        .map(|historical| decode_device_input(&historical.device))
        .collect::<Result<_>>()?;

    decoded_devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    decoded_historical.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    let mut all_decoded = decoded_devices
        .iter()
        .chain(&decoded_historical)
        .collect::<Vec<_>>();
    all_decoded.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    for pair in all_decoded.windows(2) {
        if pair[0].device_id == pair[1].device_id {
            return Err(AppError::Unsupported {
                reason: format!(
                    "identity bundle has duplicate device_id `{}`",
                    pair[0].device_id
                ),
            });
        }
    }
    let by_id: BTreeMap<String, &DecodedDevice> = all_decoded
        .iter()
        .map(|d| (d.device_id.clone(), *d))
        .collect();

    let mut verified_all: BTreeMap<String, VerifiedDevice> = BTreeMap::new();
    for decoded in decoded_devices.iter().chain(&decoded_historical) {
        let parsed = DeviceCertificate::parse_body(&decoded.cert_body)?;

        if parsed.sig_public_key != decoded.sig_pub {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device `{}`: cert sig_public_key does not match top-level signingPublicKey",
                    decoded.device_id
                ),
            });
        }
        if parsed.kem_public_key != decoded.kem_pub {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device `{}`: cert kem_public_key does not match top-level kemPublicKey",
                    decoded.device_id
                ),
            });
        }
        if parsed.device_id != decoded.device_id {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device `{}`: cert device_id does not match top-level deviceId",
                    decoded.device_id
                ),
            });
        }
        if parsed.user_id != dto.user_id {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device `{}`: cert user_id `{}` does not match bundle user_id `{}`",
                    decoded.device_id, parsed.user_id, dto.user_id
                ),
            });
        }

        let signer_pub: &[u8] = if parsed.is_self_signed() {
            &decoded.sig_pub
        } else {
            let signer =
                by_id
                    .get(&parsed.signer_device_id)
                    .ok_or_else(|| AppError::Unsupported {
                        reason: format!(
                            "device `{}`: cert signer `{}` is not in the bundle",
                            decoded.device_id, parsed.signer_device_id
                        ),
                    })?;
            &signer.sig_pub
        };

        let cert = verify_certificate(&decoded.cert_body, &decoded.cert_signature, signer_pub)?;

        verified_all.insert(
            decoded.device_id.clone(),
            VerifiedDevice {
                certificate: cert,
                sig_public_key: decoded.sig_pub.clone(),
                certificate_body: decoded.cert_body.clone(),
                certificate_signature: decoded.cert_signature.clone(),
            },
        );
    }

    let active_device_ids = decoded_devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let verified = verified_all
        .iter()
        .filter(|(device_id, _)| active_device_ids.contains(device_id.as_str()))
        .map(|(device_id, device)| (device_id.clone(), device.clone()))
        .collect::<BTreeMap<_, _>>();
    let required_historical_ids = required_historical_ancestry(&verified, &verified_all)?;
    let verified_historical = required_historical_ids
        .into_iter()
        .filter_map(|device_id| {
            verified_all
                .get(&device_id)
                .map(|device| (device_id, device.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    validate_certificate_graph(&verified, &verified_historical)?;

    let list_signer_device = verified
        .get(&dto.device_list.signer_device_id)
        .ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "device list signer `{}` is not in the bundle",
                dto.device_list.signer_device_id
            ),
        })?;
    let signed_list = verify_signed_device_list(
        &list_body,
        &list_signature,
        &list_signer_device.sig_public_key,
    )?;

    if signed_list.signer_device_id != dto.device_list.signer_device_id {
        return Err(AppError::Unsupported {
            reason: "device list body signer does not match top-level signerDeviceId".to_owned(),
        });
    }
    if signed_list.generation != dto.device_list.generation {
        return Err(AppError::Unsupported {
            reason: "device list body generation does not match top-level generation".to_owned(),
        });
    }
    if signed_list.issued_at_ms != dto.device_list.issued_at_ms {
        return Err(AppError::Unsupported {
            reason: "device list body issued_at_ms does not match top-level issuedAtMs".to_owned(),
        });
    }
    if signed_list.expires_at_ms != dto.device_list.expires_at_ms {
        return Err(AppError::Unsupported {
            reason: "device list body expires_at_ms does not match top-level expiresAtMs"
                .to_owned(),
        });
    }
    if signed_list.user_id != dto.user_id {
        return Err(AppError::Unsupported {
            reason: "device list body user_id does not match top-level userId".to_owned(),
        });
    }

    let listed_device_ids = signed_list
        .entries
        .iter()
        .map(|entry| entry.device_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let bundled_device_ids = verified
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if bundled_device_ids != listed_device_ids {
        return Err(AppError::Unsupported {
            reason: "identity bundle devices must exactly match the signed device list".to_owned(),
        });
    }

    for entry in &signed_list.entries {
        let device = verified
            .get(&entry.device_id)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!(
                    "device list references `{}` which is not in the bundle",
                    entry.device_id
                ),
            })?;
        if device.certificate.signer_device_id != entry.signer_device_id {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device `{}`: cert signer `{}` disagrees with list entry signer `{}`",
                    entry.device_id, device.certificate.signer_device_id, entry.signer_device_id
                ),
            });
        }
    }

    Ok(IdentityBundleView {
        user_id: dto.user_id.clone(),
        identity_revision: 1,
        identity_incarnation_id: uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_00ff),
        signed_list,
        list_body,
        list_signature,
        devices: verified,
        historical_devices: verified_historical,
    })
}

fn required_historical_ancestry(
    active: &BTreeMap<String, VerifiedDevice>,
    all: &BTreeMap<String, VerifiedDevice>,
) -> Result<std::collections::BTreeSet<String>> {
    let mut required = std::collections::BTreeSet::new();
    for device_id in active.keys() {
        let mut current = device_id.as_str();
        let mut path = std::collections::BTreeSet::new();
        loop {
            if !path.insert(current) {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "device certificate signer graph contains a cycle at `{current}`"
                    ),
                });
            }
            let device = all.get(current).ok_or_else(|| AppError::Unsupported {
                reason: format!("device certificate signer graph is disconnected at `{current}`"),
            })?;
            if !active.contains_key(current) {
                required.insert(current.to_owned());
            }
            let signer = device.certificate.signer_device_id.as_str();
            if signer == current {
                break;
            }
            current = signer;
        }
    }
    Ok(required)
}

fn validate_certificate_graph(
    active: &BTreeMap<String, VerifiedDevice>,
    historical: &BTreeMap<String, VerifiedDevice>,
) -> Result<()> {
    let all = active.iter().chain(historical);
    let by_id = all
        .map(|(device_id, device)| (device_id.as_str(), device))
        .collect::<BTreeMap<_, _>>();

    for device_id in active.keys() {
        let mut current = device_id.as_str();
        let mut path = std::collections::BTreeSet::new();
        loop {
            if !path.insert(current) {
                return Err(AppError::Unsupported {
                    reason: format!(
                        "device certificate signer graph contains a cycle at `{current}`"
                    ),
                });
            }
            let device = by_id.get(current).ok_or_else(|| AppError::Unsupported {
                reason: format!("device certificate signer graph is disconnected at `{current}`"),
            })?;
            let signer = device.certificate.signer_device_id.as_str();
            if signer == current {
                break;
            }
            current = signer;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct DecodedDevice {
    device_id: String,
    cert_body: Vec<u8>,
    cert_signature: Vec<u8>,
    sig_pub: Vec<u8>,
    kem_pub: Vec<u8>,
}

fn decode_device_input(device: &IdentityDeviceInput) -> Result<DecodedDevice> {
    Ok(DecodedDevice {
        device_id: device.device_id.clone(),
        cert_body: decode_b64(&device.certificate, "device.certificate")?,
        cert_signature: decode_b64(&device.certificate_signature, "device.certificateSignature")?,
        sig_pub: decode_b64(&device.signing_public_key, "device.signingPublicKey")?,
        kem_pub: decode_b64(&device.kem_public_key, "device.kemPublicKey")?,
    })
}

fn decode_b64(value: &str, field: &'static str) -> Result<Vec<u8>> {
    BASE64
        .decode(value)
        .map_err(|e| AppError::InvalidBackendData {
            field: field.to_owned(),
            reason: format!("base64 decode failed: {e}"),
        })
}
