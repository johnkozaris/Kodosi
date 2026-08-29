use aws_lc_rs::signature::{ML_DSA_65, PqdsaKeyPair, VerificationAlgorithm};

#[cfg(test)]
use aws_lc_rs::signature::KeyPair;

#[cfg(test)]
use super::wire_codec::MAX_FIELD_LEN;
use super::wire_codec::{
    LpReader, LpWriter, MAX_SIGNED_DEVICE_LIST_BODY_LEN, ML_DSA_65_SIGNATURE_LEN, decode_expiry,
    encode_expiry, validate_canonical_device_id, validate_canonical_user_id, validate_timestamps,
};
use crate::{AppError, Result};
use kodosi_domain::domain_tags::DEVICE_LIST_V1;

const DEVICE_LIST_CONTEXT: &str = "device list";
pub const MAX_ENTRIES: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceListEntry {
    pub(crate) device_id: String,
    pub(crate) signer_device_id: String,
}

impl DeviceListEntry {
    fn validate_required_fields(&self) -> Result<()> {
        validate_canonical_device_id(DEVICE_LIST_CONTEXT, "device_id", &self.device_id)?;
        validate_canonical_device_id(
            DEVICE_LIST_CONTEXT,
            "entry signer_device_id",
            &self.signer_device_id,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SignedDeviceList {
    pub(crate) user_id: String,
    pub(crate) generation: u64,
    pub(crate) entries: Vec<DeviceListEntry>,
    pub(crate) signer_device_id: String,
    pub(crate) issued_at_ms: u64,
    pub(crate) expires_at_ms: Option<u64>,
}

impl SignedDeviceList {
    pub(crate) fn serialize_body(&self) -> Result<Vec<u8>> {
        self.validate_required_fields()?;
        if self.entries.len() > MAX_ENTRIES as usize {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device list: {} entries exceeds MAX_ENTRIES ({MAX_ENTRIES})",
                    self.entries.len()
                ),
            });
        }
        let entry_count = u32::try_from(self.entries.len()).map_err(|_| AppError::Unsupported {
            reason: "device list: entry count exceeds u32".to_owned(),
        })?;

        let mut writer = LpWriter::with_capacity(
            DEVICE_LIST_CONTEXT,
            4 + self.user_id.len()
                + 8
                + 4
                + self
                    .entries
                    .iter()
                    .map(|e| 8 + e.device_id.len() + e.signer_device_id.len())
                    .sum::<usize>()
                + 4
                + self.signer_device_id.len()
                + 16,
        );
        writer.write_lp_str(&self.user_id)?;
        if self.generation == 0 {
            return Err(AppError::Unsupported {
                reason: "device list: generation must be >= 1".to_owned(),
            });
        }
        writer.write_u64_be(self.generation);
        writer.write_u32_be(entry_count);
        for entry in &self.entries {
            writer.write_lp_str(&entry.device_id)?;
            writer.write_lp_str(&entry.signer_device_id)?;
        }
        writer.write_lp_str(&self.signer_device_id)?;
        writer.write_u64_be(self.issued_at_ms);
        writer.write_u64_be(encode_expiry(self.expires_at_ms));
        let body = writer.finish();
        if body.len() > MAX_SIGNED_DEVICE_LIST_BODY_LEN {
            return Err(AppError::Unsupported {
                reason: format!(
                    "{DEVICE_LIST_CONTEXT}: body length {} exceeds MAX_SIGNED_DEVICE_LIST_BODY_LEN ({MAX_SIGNED_DEVICE_LIST_BODY_LEN})",
                    body.len()
                ),
            });
        }
        Ok(body)
    }

    pub(crate) fn parse_body(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SIGNED_DEVICE_LIST_BODY_LEN {
            return Err(AppError::Unsupported {
                reason: format!(
                    "{DEVICE_LIST_CONTEXT}: body length {} exceeds MAX_SIGNED_DEVICE_LIST_BODY_LEN ({MAX_SIGNED_DEVICE_LIST_BODY_LEN})",
                    bytes.len()
                ),
            });
        }
        let mut cursor = LpReader::new(DEVICE_LIST_CONTEXT, bytes);
        let user_id = cursor.read_lp_str()?;
        let generation = cursor.read_u64_be()?;
        if generation == 0 {
            return Err(AppError::Unsupported {
                reason: "device list: generation must be >= 1".to_owned(),
            });
        }
        let entry_count = cursor.read_u32_be()?;
        if entry_count > MAX_ENTRIES {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device list: entry_count {entry_count} exceeds MAX_ENTRIES ({MAX_ENTRIES})"
                ),
            });
        }
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            entries.push(DeviceListEntry {
                device_id: cursor.read_lp_str()?,
                signer_device_id: cursor.read_lp_str()?,
            });
        }
        let signer_device_id = cursor.read_lp_str()?;
        let issued_at_ms = cursor.read_u64_be()?;
        let expires_raw = cursor.read_u64_be()?;
        cursor.expect_consumed()?;

        let list = Self {
            user_id,
            generation,
            entries,
            signer_device_id,
            issued_at_ms,
            expires_at_ms: decode_expiry(expires_raw),
        };
        list.validate_required_fields()?;

        let signer_in_list = list
            .entries
            .iter()
            .any(|entry| entry.device_id == list.signer_device_id);
        if !signer_in_list {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device list signer {} is not present in the list entries",
                    list.signer_device_id
                ),
            });
        }

        Ok(list)
    }

    fn validate_required_fields(&self) -> Result<()> {
        validate_timestamps(DEVICE_LIST_CONTEXT, self.issued_at_ms, self.expires_at_ms)?;
        validate_canonical_user_id(DEVICE_LIST_CONTEXT, &self.user_id)?;
        validate_canonical_device_id(
            DEVICE_LIST_CONTEXT,
            "list signer_device_id",
            &self.signer_device_id,
        )?;
        validate_entries(&self.entries)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SignedDeviceListEnvelope {
    #[cfg(test)]
    pub(crate) list: SignedDeviceList,
    pub(crate) body_bytes: Vec<u8>,
    pub(crate) signature: Vec<u8>,
}

pub(crate) fn build_bootstrap_list(
    user_id: &str,
    own_device_id: &str,
    own_signing_keypair: &PqdsaKeyPair,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<SignedDeviceListEnvelope> {
    let list = SignedDeviceList {
        user_id: user_id.to_owned(),
        generation: 1,
        entries: vec![DeviceListEntry {
            device_id: own_device_id.to_owned(),
            signer_device_id: own_device_id.to_owned(),
        }],
        signer_device_id: own_device_id.to_owned(),
        issued_at_ms,
        expires_at_ms,
    };
    sign_list(&list, own_signing_keypair)
}

pub(crate) fn build_replacement_list(
    user_id: &str,
    previous_generation: u64,
    previous_entries: &[DeviceListEntry],
    next_entries: Vec<DeviceListEntry>,
    signer_device_id: &str,
    signer_keypair: &PqdsaKeyPair,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<SignedDeviceListEnvelope> {
    validate_entries(previous_entries)?;
    validate_entries(&next_entries)?;
    validate_canonical_device_id(
        DEVICE_LIST_CONTEXT,
        "list signer_device_id",
        signer_device_id,
    )?;
    if next_entries.is_empty() {
        return Err(AppError::Unsupported {
            reason: "device list cannot be empty — at least one device must remain".to_owned(),
        });
    }
    if !previous_entries
        .iter()
        .any(|e| e.device_id == signer_device_id)
    {
        return Err(AppError::Unsupported {
            reason: format!(
                "list signer {signer_device_id} is not in the previous list — cannot replace"
            ),
        });
    }
    if !next_entries.iter().any(|e| e.device_id == signer_device_id) {
        return Err(AppError::Unsupported {
            reason: format!(
                "list signer {signer_device_id} is not in the new entry set — would invalidate the list"
            ),
        });
    }
    let generation = previous_generation
        .checked_add(1)
        .ok_or_else(|| AppError::Unsupported {
            reason: "device list generation overflow".to_owned(),
        })?;
    let list = SignedDeviceList {
        user_id: user_id.to_owned(),
        generation,
        entries: next_entries,
        signer_device_id: signer_device_id.to_owned(),
        issued_at_ms,
        expires_at_ms,
    };
    sign_list(&list, signer_keypair)
}

pub(crate) fn verify_signed_device_list(
    body_bytes: &[u8],
    signature: &[u8],
    signer_sig_pubkey: &[u8],
) -> Result<SignedDeviceList> {
    if signature.len() != ML_DSA_65_SIGNATURE_LEN {
        return Err(AppError::Unsupported {
            reason: format!(
                "{DEVICE_LIST_CONTEXT}: signature length {} does not equal ML_DSA_65_SIGNATURE_LEN ({ML_DSA_65_SIGNATURE_LEN})",
                signature.len()
            ),
        });
    }
    let mut payload = Vec::with_capacity(DEVICE_LIST_V1.len() + body_bytes.len());
    payload.extend_from_slice(DEVICE_LIST_V1);
    payload.extend_from_slice(body_bytes);

    ML_DSA_65
        .verify_sig(signer_sig_pubkey, &payload, signature)
        .map_err(|_| AppError::Unsupported {
            reason: "device list signature verification failed".to_owned(),
        })?;

    SignedDeviceList::parse_body(body_bytes)
}

fn sign_list(list: &SignedDeviceList, signer: &PqdsaKeyPair) -> Result<SignedDeviceListEnvelope> {
    let body_bytes = list.serialize_body()?;
    #[cfg(test)]
    let list = SignedDeviceList::parse_body(&body_bytes)?;
    #[cfg(not(test))]
    SignedDeviceList::parse_body(&body_bytes)?;
    let mut payload = Vec::with_capacity(DEVICE_LIST_V1.len() + body_bytes.len());
    payload.extend_from_slice(DEVICE_LIST_V1);
    payload.extend_from_slice(&body_bytes);

    let mut signature = vec![0u8; ML_DSA_65_SIGNATURE_LEN];
    let sig_len = signer
        .sign(&payload, &mut signature)
        .map_err(|_| AppError::Unsupported {
            reason: "device list signing failed".to_owned(),
        })?;
    signature.truncate(sig_len);

    Ok(SignedDeviceListEnvelope {
        #[cfg(test)]
        list,
        body_bytes,
        signature,
    })
}

fn validate_entries(entries: &[DeviceListEntry]) -> Result<()> {
    let mut device_ids = std::collections::BTreeSet::new();
    for entry in entries {
        entry.validate_required_fields()?;
        if !device_ids.insert(entry.device_id.as_str()) {
            return Err(AppError::Unsupported {
                reason: format!(
                    "device list contains duplicate device_id {}",
                    entry.device_id
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "signed_device_list_tests.rs"]
mod tests;
