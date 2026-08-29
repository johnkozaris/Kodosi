mod actor;
mod persistence;
mod schema;
mod store;
mod verifier;

pub(crate) use actor::DeviceListPinStoreHandle;
pub(crate) use schema::DeviceListPin;
pub(crate) use store::{DeviceListPinStore, IdentityLifecycleApplyOutcome};

pub(crate) use super::identity_bundle_view::{
    BreakReason, IdentityBundleInput, IdentityBundleView, PinContext, PinVerdict,
};

pub(super) use base64::engine::general_purpose::STANDARD as BASE64;

pub(crate) fn identity_bundle_fingerprint(bundle: &IdentityBundleInput) -> crate::Result<[u8; 32]> {
    let value = serde_json::to_value(bundle).map_err(crate::AppError::Json)?;
    let canonical = serde_json::to_vec(&value).map_err(crate::AppError::Json)?;
    aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, &canonical)
        .as_ref()
        .try_into()
        .map_err(|_| crate::AppError::Unsupported {
            reason: "identity bundle digest length was not SHA-256".to_owned(),
        })
}

#[cfg(test)]
use crate::{AppError, Result};
#[cfg(test)]
use base64::Engine;
#[cfg(test)]
use std::fs;

#[cfg(test)]
#[path = "device_list_pin_store_tests/mod.rs"]
mod tests;
