pub mod device_cert;
pub mod keys;
pub(crate) mod link_code;
pub mod oidc;
pub mod pins;
pub mod signed_device_list;
pub mod storage;
mod wire_codec;

pub(crate) use wire_codec::{ML_DSA_65_PUBLIC_KEY_LEN, ML_KEM_768_PUBLIC_KEY_LEN};

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
