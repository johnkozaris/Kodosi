use security_framework::passwords::{
    PasswordOptions, delete_generic_password_options, generic_password,
    set_generic_password_options,
};
use zeroize::Zeroizing;

use crate::{AppError, Result};

const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34018;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum KeychainLoadResult {
    Loaded(Zeroizing<String>),
    Missing,
    MissingEntitlement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeychainStoreResult {
    Stored,
    MissingEntitlement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeychainDeleteResult {
    Deleted,
    MissingEntitlement,
}

fn options(service: &str, account: &str) -> PasswordOptions {
    let mut opts = PasswordOptions::new_generic_password(service, account);
    opts.use_protected_keychain();
    opts
}

pub(super) fn store_password(
    service: &str,
    account: &str,
    password: &str,
) -> Result<KeychainStoreResult> {
    match set_generic_password_options(password.as_bytes(), options(service, account)) {
        Ok(()) => Ok(KeychainStoreResult::Stored),
        Err(e) if e.code() == ERR_SEC_MISSING_ENTITLEMENT => {
            Ok(KeychainStoreResult::MissingEntitlement)
        }
        Err(e) => Err(AppError::Keychain {
            reason: format!("failed to store credential: {e}"),
        }),
    }
}

pub(super) fn load_password(service: &str, account: &str) -> Result<KeychainLoadResult> {
    match generic_password(options(service, account)) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(value) => Ok(KeychainLoadResult::Loaded(Zeroizing::new(value))),
            Err(error) => {
                let utf8_error = error.utf8_error();
                let _bytes = Zeroizing::new(error.into_bytes());
                Err(AppError::Keychain {
                    reason: format!("stored credential is not valid UTF-8: {utf8_error}"),
                })
            }
        },
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(KeychainLoadResult::Missing),
        Err(e) if e.code() == ERR_SEC_MISSING_ENTITLEMENT => {
            Ok(KeychainLoadResult::MissingEntitlement)
        }
        Err(e) => Err(AppError::Keychain {
            reason: format!("failed to load credential: {e}"),
        }),
    }
}

pub(super) fn delete_password(service: &str, account: &str) -> Result<KeychainDeleteResult> {
    match delete_generic_password_options(options(service, account)) {
        Ok(()) => Ok(KeychainDeleteResult::Deleted),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(KeychainDeleteResult::Deleted),
        Err(e) if e.code() == ERR_SEC_MISSING_ENTITLEMENT => {
            Ok(KeychainDeleteResult::MissingEntitlement)
        }
        Err(e) => Err(AppError::Keychain {
            reason: format!("failed to delete credential: {e}"),
        }),
    }
}
