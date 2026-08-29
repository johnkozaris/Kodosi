use crate::{AppError, Result};

pub const MAX_FIELD_LEN: u32 = 65_536;
pub(crate) const USER_ID_LEN: usize = 36;
pub(crate) const DEVICE_ID_MAX_UTF16_CODE_UNITS: usize = 256;
pub(crate) const DEVICE_LABEL_MAX_UTF16_CODE_UNITS: usize = 128;
pub(crate) const ML_KEM_768_PUBLIC_KEY_LEN: usize = 1184;
pub(crate) const ML_DSA_65_PUBLIC_KEY_LEN: usize = 1952;
pub(crate) const ML_DSA_65_SIGNATURE_LEN: usize = 3309;
pub(crate) const MAX_DEVICE_CERTIFICATE_BODY_LEN: usize = 5772;
pub(crate) const MAX_SIGNED_DEVICE_LIST_BODY_LEN: usize = 527_432;
pub const NO_EXPIRY_SENTINEL: u64 = 0;
pub const MAX_UNIX_TIME_MILLISECONDS: u64 = 253_402_300_799_999;

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
        let len_u32 = u32::try_from(bytes.len()).map_err(|_| AppError::Unsupported {
            reason: format!("{} field exceeds u32 length prefix", self.context),
        })?;
        if len_u32 > MAX_FIELD_LEN {
            return Err(AppError::Unsupported {
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
        let end = self
            .pos
            .checked_add(4)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!("{}: u32 length overflow", self.context),
            })?;
        if end > self.bytes.len() {
            return Err(AppError::Unsupported {
                reason: format!("{}: truncated u32", self.context),
            });
        }
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.bytes[self.pos..end]);
        self.pos = end;
        Ok(u32::from_be_bytes(buf))
    }

    pub(super) fn read_u64_be(&mut self) -> Result<u64> {
        let end = self
            .pos
            .checked_add(8)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!("{}: u64 length overflow", self.context),
            })?;
        if end > self.bytes.len() {
            return Err(AppError::Unsupported {
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
            return Err(AppError::Unsupported {
                reason: format!(
                    "{}: field length {len_u32} exceeds MAX_FIELD_LEN ({MAX_FIELD_LEN})",
                    self.context
                ),
            });
        }
        let len = len_u32 as usize;
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| AppError::Unsupported {
                reason: format!("{}: field length overflow", self.context),
            })?;
        if end > self.bytes.len() {
            return Err(AppError::Unsupported {
                reason: format!("{}: truncated field", self.context),
            });
        }
        let out = self.bytes[self.pos..end].to_vec();
        self.pos = end;
        Ok(out)
    }

    pub(super) fn read_lp_str(&mut self) -> Result<String> {
        let bytes = self.read_lp_bytes()?;
        String::from_utf8(bytes).map_err(|_| AppError::Unsupported {
            reason: format!("{}: invalid UTF-8 in string field", self.context),
        })
    }

    pub(super) fn expect_consumed(&self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(AppError::Unsupported {
                reason: format!("{}: trailing bytes after parse", self.context),
            })
        }
    }
}

pub(super) fn validate_canonical_user_id(context: &'static str, value: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value).map_err(|_| AppError::Unsupported {
        reason: format!("{context}: user_id must be a canonical lowercase UUID"),
    })?;
    if value.len() != USER_ID_LEN || parsed.hyphenated().to_string() != value {
        return Err(AppError::Unsupported {
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
        return Err(AppError::Unsupported {
            reason: format!(
                "{context}: {field} must be canonical non-blank text of at most {DEVICE_ID_MAX_UTF16_CODE_UNITS} UTF-16 code units"
            ),
        });
    }
    Ok(())
}

pub(super) fn reject_zero_expiry(context: &'static str, expires_at_ms: Option<u64>) -> Result<()> {
    if expires_at_ms == Some(0) {
        return Err(AppError::Unsupported {
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
            return Err(AppError::Unsupported {
                reason: format!(
                    "{context}: {field} exceeds MAX_UNIX_TIME_MILLISECONDS ({MAX_UNIX_TIME_MILLISECONDS})"
                ),
            });
        }
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
    use proptest::prelude::*;

    fn proptest_config() -> ProptestConfig {
        let mut config = ProptestConfig::with_cases(128);
        config.failure_persistence = None;
        config
    }

    #[test]
    fn canonical_user_id_rejects_noncanonical_uuid_text() {
        assert!(validate_canonical_user_id("test", "01900000-0000-7000-8000-000000000001").is_ok());
        assert!(
            validate_canonical_user_id("test", "01900000-0000-7000-8000-00000000000A").is_err()
        );
        assert!(validate_canonical_user_id("test", "01900000000070008000000000000001").is_err());
        assert!(validate_canonical_user_id("test", "user-1").is_err());
    }

    #[test]
    fn canonical_device_id_counts_utf16_code_units() {
        let accepted = "😀".repeat(DEVICE_ID_MAX_UTF16_CODE_UNITS / 2);
        let rejected = format!("{accepted}a");
        assert!(validate_canonical_device_id("test", "device_id", &accepted).is_ok());
        assert!(validate_canonical_device_id("test", "device_id", &rejected).is_err());
    }

    proptest! {
        #![proptest_config(proptest_config())]

        #[test]
        fn byte_fields_round_trip(
            fields in prop::collection::vec(
                prop::collection::vec(any::<u8>(), 0..=1024),
                0..=16,
            ),
        ) {
            let mut writer = LpWriter::with_capacity("test wire", 0);
            for field in &fields {
                writer.write_lp_bytes(field).expect("generated field writes");
            }

            let bytes = writer.finish();
            let mut reader = LpReader::new("test wire", &bytes);
            for expected in &fields {
                let actual = reader.read_lp_bytes().expect("generated field reads");
                prop_assert_eq!(&actual, expected);
            }
            prop_assert!(reader.expect_consumed().is_ok());
        }

        #[test]
        fn string_fields_round_trip(
            fields in prop::collection::vec("[ -~]{0,64}", 0..=16),
        ) {
            let mut writer = LpWriter::with_capacity("test wire", 0);
            for field in &fields {
                writer.write_lp_str(field).expect("generated string writes");
            }

            let bytes = writer.finish();
            let mut reader = LpReader::new("test wire", &bytes);
            for expected in &fields {
                let actual = reader.read_lp_str().expect("generated string reads");
                prop_assert_eq!(&actual, expected);
            }
            prop_assert!(reader.expect_consumed().is_ok());
        }

        #[test]
        fn expiry_codec_preserves_none_and_representable_values(
            expires_at_ms in prop::option::of(1u64..=MAX_UNIX_TIME_MILLISECONDS)
        ) {
            let encoded = encode_expiry(expires_at_ms);

            prop_assert_eq!(decode_expiry(encoded), expires_at_ms);
            prop_assert!(validate_timestamps("test wire", 0, expires_at_ms).is_ok());
        }
    }

    #[test]
    fn reader_rejects_declared_field_above_max_without_allocation() {
        let mut body = Vec::new();
        body.extend_from_slice(&(MAX_FIELD_LEN + 1).to_be_bytes());

        let mut reader = LpReader::new("test wire", &body);
        assert!(reader.read_lp_bytes().is_err());
    }

    #[test]
    fn writer_rejects_field_above_max() {
        let oversized = vec![0u8; MAX_FIELD_LEN as usize + 1];
        let mut writer = LpWriter::with_capacity("test wire", 0);

        assert!(writer.write_lp_bytes(&oversized).is_err());
    }

    #[test]
    fn reader_rejects_invalid_utf8_string() {
        let mut writer = LpWriter::with_capacity("test wire", 0);
        writer.write_lp_bytes(&[0xC0, 0xC0]).unwrap();
        let bytes = writer.finish();

        let mut reader = LpReader::new("test wire", &bytes);
        assert!(reader.read_lp_str().is_err());
    }

    #[test]
    fn zero_expiry_is_reserved_for_none() {
        assert_eq!(encode_expiry(None), NO_EXPIRY_SENTINEL);
        assert_eq!(decode_expiry(NO_EXPIRY_SENTINEL), None);
        assert!(validate_timestamps("test wire", 0, Some(0)).is_err());
    }

    #[test]
    fn timestamps_above_cross_stack_maximum_are_rejected() {
        let overflow = MAX_UNIX_TIME_MILLISECONDS + 1;
        assert!(validate_timestamps("test wire", overflow, None).is_err());
        assert!(validate_timestamps("test wire", 0, Some(overflow)).is_err());
    }
}
