pub(crate) mod access_token_resolver;
pub(crate) mod device_cert;
pub(crate) mod device_flow;
pub(crate) mod device_keys;
pub(crate) mod device_link;
pub(crate) mod device_list_pin_store;
pub(crate) mod identity_bundle_view;
mod platform_secrets;
pub(crate) mod signed_device_list;
pub(crate) mod stored_auth;
pub(crate) mod token_refresh;
pub(crate) mod token_store;
mod wire_codec;

pub use signed_device_list::MAX_ENTRIES as IDENTITY_MAX_ENTRIES;
pub(crate) use wire_codec::{
    DEVICE_ID_MAX_UTF16_CODE_UNITS as IDENTITY_DEVICE_ID_MAX_UTF16_CODE_UNITS,
    DEVICE_LABEL_MAX_UTF16_CODE_UNITS as IDENTITY_DEVICE_LABEL_MAX_UTF16_CODE_UNITS,
    MAX_DEVICE_CERTIFICATE_BODY_LEN as IDENTITY_MAX_DEVICE_CERTIFICATE_BODY_LEN,
    MAX_SIGNED_DEVICE_LIST_BODY_LEN as IDENTITY_MAX_SIGNED_DEVICE_LIST_BODY_LEN,
    ML_DSA_65_PUBLIC_KEY_LEN as IDENTITY_ML_DSA_65_PUBLIC_KEY_LEN,
    ML_DSA_65_SIGNATURE_LEN as IDENTITY_ML_DSA_65_SIGNATURE_LEN,
    ML_KEM_768_PUBLIC_KEY_LEN as IDENTITY_ML_KEM_768_PUBLIC_KEY_LEN,
};
pub use wire_codec::{
    MAX_FIELD_LEN as IDENTITY_MAX_FIELD_LEN,
    MAX_UNIX_TIME_MILLISECONDS as IDENTITY_MAX_UNIX_TIME_MILLISECONDS,
    NO_EXPIRY_SENTINEL as IDENTITY_NO_EXPIRY_SENTINEL,
};
