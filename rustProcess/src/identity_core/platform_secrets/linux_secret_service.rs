use keyring::{Entry, Error as KeyringError};
use zeroize::Zeroizing;

use crate::{AppError, Result};

pub(super) enum LoadResult {
    Loaded(Zeroizing<String>),
    Missing,
    Unavailable,
}

pub(super) enum StoreResult {
    Stored,
    Unavailable,
}

pub(super) enum DeleteResult {
    Deleted,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorDisposition {
    Missing,
    Unavailable,
    Failed,
}

fn disposition(error: &KeyringError) -> ErrorDisposition {
    match error {
        KeyringError::NoEntry => ErrorDisposition::Missing,
        KeyringError::NoDefaultStore | KeyringError::NoStorageAccess(_) => {
            ErrorDisposition::Unavailable
        }
        _ => ErrorDisposition::Failed,
    }
}

fn operation_error(operation: &str, error: &KeyringError) -> AppError {
    AppError::Unsupported {
        reason: format!("Linux Secret Service {operation} failed: {error}"),
    }
}

pub(super) fn load_password(service_name: &str, account_label: &str) -> Result<LoadResult> {
    let entry = match Entry::new(service_name, account_label) {
        Ok(entry) => entry,
        Err(error) if disposition(&error) == ErrorDisposition::Unavailable => {
            return Ok(LoadResult::Unavailable);
        }
        Err(error) => return Err(operation_error("entry creation", &error)),
    };
    match entry.get_password() {
        Ok(payload) => Ok(LoadResult::Loaded(Zeroizing::new(payload))),
        Err(error) => match disposition(&error) {
            ErrorDisposition::Missing => Ok(LoadResult::Missing),
            ErrorDisposition::Unavailable => Ok(LoadResult::Unavailable),
            ErrorDisposition::Failed => Err(operation_error("load", &error)),
        },
    }
}

pub(super) fn store_password(
    service_name: &str,
    account_label: &str,
    payload: &str,
) -> Result<StoreResult> {
    let entry = match Entry::new(service_name, account_label) {
        Ok(entry) => entry,
        Err(error) if disposition(&error) == ErrorDisposition::Unavailable => {
            return Ok(StoreResult::Unavailable);
        }
        Err(error) => return Err(operation_error("entry creation", &error)),
    };
    match entry.set_password(payload) {
        Ok(()) => Ok(StoreResult::Stored),
        Err(error) if disposition(&error) == ErrorDisposition::Unavailable => {
            Ok(StoreResult::Unavailable)
        }
        Err(error) => Err(operation_error("store", &error)),
    }
}

pub(super) fn delete_password(service_name: &str, account_label: &str) -> Result<DeleteResult> {
    let entry = match Entry::new(service_name, account_label) {
        Ok(entry) => entry,
        Err(error) if disposition(&error) == ErrorDisposition::Unavailable => {
            return Ok(DeleteResult::Unavailable);
        }
        Err(error) => return Err(operation_error("entry creation", &error)),
    };
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(DeleteResult::Deleted),
        Err(error) if disposition(&error) == ErrorDisposition::Unavailable => {
            Ok(DeleteResult::Unavailable)
        }
        Err(error) => Err(operation_error("delete", &error)),
    }
}

pub(super) fn unavailable_error(operation: &str) -> AppError {
    AppError::Unsupported {
        reason: format!("Linux Secret Service is unavailable during {operation}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorDisposition, disposition};
    use keyring::Error;

    #[test]
    fn missing_and_unavailable_errors_remain_distinct() {
        assert_eq!(disposition(&Error::NoEntry), ErrorDisposition::Missing);
        assert_eq!(
            disposition(&Error::NoDefaultStore),
            ErrorDisposition::Unavailable
        );
        assert_eq!(
            disposition(&Error::Invalid("service".to_owned(), "invalid".to_owned())),
            ErrorDisposition::Failed
        );
    }
}
