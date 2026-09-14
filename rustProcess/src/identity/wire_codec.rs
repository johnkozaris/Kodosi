use crate::network::{Error, Result};

pub(super) const MAX_FIELD_LEN: u32 = 65_536;
pub(crate) const USER_ID_LEN: usize = 36;
pub(crate) const DEVICE_ID_MAX_UTF16_CODE_UNITS: usize = 256;
pub(crate) const DEVICE_LABEL_MAX_UTF16_CODE_UNITS: usize = 128;
pub(crate) const ML_KEM_768_PUBLIC_KEY_LEN: usize = 1184;
pub(crate) const ML_DSA_65_PUBLIC_KEY_LEN: usize = 1952;
pub(crate) const ML_DSA_65_SIGNATURE_LEN: usize = 3309;
pub(crate) const MAX_DEVICE_CERTIFICATE_BODY_LEN: usize = 5772;
pub(crate) const MAX_SIGNED_DEVICE_LIST_BODY_LEN: usize = 527_432;
pub(super) const NO_EXPIRY_SENTINEL: u64 = 0;
pub(super) const MAX_UNIX_TIME_MILLISECONDS: u64 = 253_402_300_799_999;

pub(super) struct LpWriter {
    context: &'static str,
    out: Vec<u8>,
}

impl LpWriter {
    pub(super) fn with_capacity(context: &'static str, capacity: usize) -> Self {
        Self {
            context,
            out: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn write_lp_str(&mut self, value: &str) -> Result<()> {
        self.write_lp_bytes(value.as_bytes())
    }

    pub(super) fn write_lp_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let len_u32 = u32::try_from(bytes.len()).map_err(|_| Error::Invalid {
            reason: format!("{} field exceeds u32 length prefix", self.context),
        })?;
        if len_u32 > MAX_FIELD_LEN {
            return Err(Error::Invalid {
                reason: format!(
                    "{} field length {len_u32} exceeds MAX_FIELD_LEN ({MAX_FIELD_LEN})",
                    self.context
                ),
            });
        }
        self.out.extend_from_slice(&len_u32.to_be_bytes());
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    pub(super) fn write_u32_be(&mut self, value: u32) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    pub(super) fn write_u64_be(&mut self, value: u64) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    pub(super) fn finish(self) -> Vec<u8> {
        self.out
    }
}

pub(super) struct LpReader<'a> {
    context: &'static str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> LpReader<'a> {
    pub(super) fn new(context: &'static str, bytes: &'a [u8]) -> Self {
        Self {
            context,
            bytes,
            pos: 0,
        }
    }

    pub(super) fn read_u32_be(&mut self) -> Result<u32> {
        let end = self.pos.checked_add(4).ok_or_else(|| Error::Invalid {
            reason: format!("{}: u32 length overflow", self.context),
        })?;
        if end > self.bytes.len() {
            return Err(Error::Invalid {
                reason: format!("{}: truncated u32", self.context),
            });
        }
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.bytes[self.pos..end]);
        self.pos = end;
        Ok(u32::from_be_bytes(buf))
    }

    pub(super) fn read_u64_be(&mut self) -> Result<u64> {
        let end = self.pos.checked_add(8).ok_or_else(|| Error::Invalid {
            reason: format!("{}: u64 length overflow", self.context),
        })?;
        if end > self.bytes.len() {
            return Err(Error::Invalid {
                reason: format!("{}: truncated u64", self.context),
            });
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.bytes[self.pos..end]);
        self.pos = end;
        Ok(u64::from_be_bytes(buf))
    }

    pub(super) fn read_lp_bytes(&mut self) -> Result<Vec<u8>> {
        let len_u32 = self.read_u32_be()?;
        if len_u32 > MAX_FIELD_LEN {
            return Err(Error::Invalid {
                reason: format!(
                    "{}: field length {len_u32} exceeds MAX_FIELD_LEN ({MAX_FIELD_LEN})",
                    self.context
                ),
            });
        }
        let len = len_u32 as usize;
        let end = self.pos.checked_add(len).ok_or_else(|| Error::Invalid {
            reason: format!("{}: field length overflow", self.context),
        })?;
        if end > self.bytes.len() {
            return Err(Error::Invalid {
                reason: format!("{}: truncated field", self.context),
            });
        }
        let out = self.bytes[self.pos..end].to_vec();
        self.pos = end;
        Ok(out)
    }

    pub(super) fn read_lp_str(&mut self) -> Result<String> {
        let bytes = self.read_lp_bytes()?;
        String::from_utf8(bytes).map_err(|_| Error::Invalid {
            reason: format!("{}: invalid UTF-8 in string field", self.context),
        })
    }

    pub(super) fn expect_consumed(&self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(Error::Invalid {
                reason: format!("{}: trailing bytes after parse", self.context),
            })
        }
    }
}

pub(super) fn validate_canonical_user_id(context: &'static str, value: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value).map_err(|_| Error::Invalid {
        reason: format!("{context}: user_id must be a canonical lowercase UUID"),
    })?;
    if value.len() != USER_ID_LEN || parsed.hyphenated().to_string() != value {
        return Err(Error::Invalid {
            reason: format!("{context}: user_id must be a canonical lowercase UUID"),
        });
    }
    Ok(())
}

pub(super) fn validate_canonical_device_id(
    context: &'static str,
    field: &str,
    value: &str,
) -> Result<()> {
    if value.trim() != value
        || value.is_empty()
        || value.encode_utf16().count() > DEVICE_ID_MAX_UTF16_CODE_UNITS
    {
        return Err(Error::Invalid {
            reason: format!(
                "{context}: {field} must be canonical non-blank text of at most {DEVICE_ID_MAX_UTF16_CODE_UNITS} UTF-16 code units"
            ),
        });
    }
    Ok(())
}

pub(super) fn reject_zero_expiry(context: &'static str, expires_at_ms: Option<u64>) -> Result<()> {
    if expires_at_ms == Some(0) {
        return Err(Error::Invalid {
            reason: format!(
                "{context}: expires_at_ms = Some(0) is reserved as the no-expiry sentinel; pass None instead"
            ),
        });
    }
    Ok(())
}

pub(super) fn validate_timestamps(
    context: &'static str,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<()> {
    reject_zero_expiry(context, expires_at_ms)?;
    for (field, value) in [
        ("issued_at_ms", Some(issued_at_ms)),
        ("expires_at_ms", expires_at_ms),
    ] {
        if value.is_some_and(|value| value > MAX_UNIX_TIME_MILLISECONDS) {
            return Err(Error::Invalid {
                reason: format!(
                    "{context}: {field} exceeds MAX_UNIX_TIME_MILLISECONDS ({MAX_UNIX_TIME_MILLISECONDS})"
                ),
            });
        }
    }
    if expires_at_ms.is_some_and(|expires| expires <= issued_at_ms) {
        return Err(Error::Invalid {
            reason: format!("{context}: expires_at_ms must be greater than issued_at_ms"),
        });
    }
    Ok(())
}

pub(super) fn encode_expiry(expires_at_ms: Option<u64>) -> u64 {
    expires_at_ms.unwrap_or(NO_EXPIRY_SENTINEL)
}

pub(super) fn decode_expiry(expires_at_ms: u64) -> Option<u64> {
    if expires_at_ms == NO_EXPIRY_SENTINEL {
        None
    } else {
        Some(expires_at_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{
        device_cert::DeviceCertificate,
        signed_device_list::{DeviceListEntry, SignedDeviceList},
    };

    #[test]
    fn certificate_and_list_share_the_backend_timestamp_contract() {
        let mut certificate = DeviceCertificate {
            user_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            device_id: "device".to_owned(),
            device_label: "Device".to_owned(),
            signer_device_id: "device".to_owned(),
            kem_public_key: vec![0; ML_KEM_768_PUBLIC_KEY_LEN],
            sig_public_key: vec![0; ML_DSA_65_PUBLIC_KEY_LEN],
            issued_at_ms: 100,
            expires_at_ms: None,
        };
        let mut list = SignedDeviceList {
            user_id: certificate.user_id.clone(),
            generation: 1,
            entries: vec![DeviceListEntry {
                device_id: "device".to_owned(),
                signer_device_id: "device".to_owned(),
            }],
            signer_device_id: "device".to_owned(),
            issued_at_ms: 100,
            expires_at_ms: None,
        };
        let cert_body = certificate.serialize_body().unwrap();
        let list_body = list.serialize_body().unwrap();
        for expiry in [
            0,
            99,
            100,
            101,
            MAX_UNIX_TIME_MILLISECONDS,
            MAX_UNIX_TIME_MILLISECONDS + 1,
        ] {
            let accepted = expiry == 0 || (expiry > 100 && expiry <= MAX_UNIX_TIME_MILLISECONDS);
            for body in [&cert_body, &list_body] {
                let mut bytes = body.clone();
                let end = bytes.len();
                bytes[end - 8..].copy_from_slice(&expiry.to_be_bytes());
                let parsed = if body.len() == cert_body.len() {
                    DeviceCertificate::parse_body(&bytes).is_ok()
                } else {
                    SignedDeviceList::parse_body(&bytes).is_ok()
                };
                assert_eq!(parsed, accepted, "expiry {expiry}");
            }
            certificate.expires_at_ms = decode_expiry(expiry);
            list.expires_at_ms = decode_expiry(expiry);
            assert_eq!(certificate.serialize_body().is_ok(), accepted);
            assert_eq!(list.serialize_body().is_ok(), accepted);
        }
        for generation in [0, 1, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
            let mut bytes = list_body.clone();
            bytes[4 + USER_ID_LEN..12 + USER_ID_LEN].copy_from_slice(&generation.to_be_bytes());
            assert_eq!(
                SignedDeviceList::parse_body(&bytes).is_ok(),
                generation > 0 && i64::try_from(generation).is_ok()
            );
        }
    }
}
