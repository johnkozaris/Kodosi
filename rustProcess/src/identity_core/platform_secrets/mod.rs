use std::sync::atomic::{AtomicU8, Ordering};

use zeroize::Zeroizing;

use crate::Result;

#[cfg(target_os = "linux")]
mod linux_secret_service;
#[cfg(target_os = "macos")]
mod macos_keychain;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlatformSecretLoadResult {
    Loaded(Zeroizing<String>),
    Missing,
    Unavailable {
        requires_existing_file_fallback: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlatformSecretStoreResult {
    Stored,
    Unavailable,
}

const SECURE_STORE_ENABLED: u8 = 0;
const SECURE_STORE_FILE_ONLY: u8 = 1;
const SECURE_STORE_UNAVAILABLE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecureStoreState {
    Enabled,
    FileOnly,
    Unavailable,
}

#[derive(Debug)]
pub(crate) struct PlatformSecretStore {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    service_name: String,
    secure_store_state: AtomicU8,
}

impl PlatformSecretStore {
    pub(crate) fn new(service_name: &str, file_dir: std::path::PathBuf) -> Self {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = service_name;
        drop(file_dir);

        Self {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            service_name: service_name.to_owned(),
            secure_store_state: AtomicU8::new(SECURE_STORE_ENABLED),
        }
    }

    pub(crate) fn load(&self, account_label: &str) -> Result<PlatformSecretLoadResult> {
        match self.secure_store_state() {
            SecureStoreState::Enabled => self.load_from_platform(account_label),
            SecureStoreState::FileOnly => Ok(Self::file_only_load_result()),
            SecureStoreState::Unavailable => Ok(Self::platform_unavailable_load_result()),
        }
    }

    pub(crate) fn store(
        &self,
        account_label: &str,
        payload: &str,
    ) -> Result<PlatformSecretStoreResult> {
        match self.secure_store_state() {
            SecureStoreState::Enabled => {}
            SecureStoreState::FileOnly => return Ok(PlatformSecretStoreResult::Unavailable),
            SecureStoreState::Unavailable => {
                #[cfg(target_os = "linux")]
                return Err(linux_secret_service::unavailable_error("store"));
                #[cfg(not(target_os = "linux"))]
                return Ok(PlatformSecretStoreResult::Unavailable);
            }
        }

        self.store_to_platform(account_label, payload)
    }

    pub(crate) fn delete(&self, account_label: &str) -> Result<()> {
        self.delete_from_platform(account_label)
    }

    pub(crate) fn disable_secure_store(&self) {
        self.secure_store_state
            .store(SECURE_STORE_FILE_ONLY, Ordering::Relaxed);
    }

    fn mark_secure_store_unavailable(&self) {
        self.secure_store_state
            .store(SECURE_STORE_UNAVAILABLE, Ordering::Relaxed);
    }

    fn secure_store_state(&self) -> SecureStoreState {
        match self.secure_store_state.load(Ordering::Relaxed) {
            SECURE_STORE_ENABLED => SecureStoreState::Enabled,
            SECURE_STORE_FILE_ONLY => SecureStoreState::FileOnly,
            _ => SecureStoreState::Unavailable,
        }
    }

    #[cfg(target_os = "macos")]
    fn load_from_platform(&self, account_label: &str) -> Result<PlatformSecretLoadResult> {
        match macos_keychain::load_password(&self.service_name, account_label) {
            Ok(macos_keychain::KeychainLoadResult::Loaded(payload)) => {
                Ok(PlatformSecretLoadResult::Loaded(payload))
            }
            Ok(macos_keychain::KeychainLoadResult::Missing) => {
                Ok(PlatformSecretLoadResult::Missing)
            }
            Ok(macos_keychain::KeychainLoadResult::MissingEntitlement) => {
                self.mark_secure_store_unavailable();
                Ok(Self::platform_unavailable_load_result())
            }
            Err(error) => Err(error),
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[cfg(not(target_os = "linux"))]
    fn load_from_platform(&self, _account_label: &str) -> Result<PlatformSecretLoadResult> {
        Ok(Self::platform_unavailable_load_result())
    }

    #[cfg(target_os = "linux")]
    fn load_from_platform(&self, account_label: &str) -> Result<PlatformSecretLoadResult> {
        match linux_secret_service::load_password(&self.service_name, account_label)? {
            linux_secret_service::LoadResult::Loaded(payload) => {
                Ok(PlatformSecretLoadResult::Loaded(payload))
            }
            linux_secret_service::LoadResult::Missing => Ok(PlatformSecretLoadResult::Missing),
            linux_secret_service::LoadResult::Unavailable => {
                self.mark_secure_store_unavailable();
                Ok(Self::platform_unavailable_load_result())
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn store_to_platform(
        &self,
        account_label: &str,
        payload: &str,
    ) -> Result<PlatformSecretStoreResult> {
        match macos_keychain::store_password(&self.service_name, account_label, payload) {
            Ok(macos_keychain::KeychainStoreResult::Stored) => {
                Ok(PlatformSecretStoreResult::Stored)
            }
            Ok(macos_keychain::KeychainStoreResult::MissingEntitlement) => {
                self.mark_secure_store_unavailable();
                Ok(PlatformSecretStoreResult::Unavailable)
            }
            Err(error) => Err(error),
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[cfg(not(target_os = "linux"))]
    fn store_to_platform(
        &self,
        _account_label: &str,
        _payload: &str,
    ) -> Result<PlatformSecretStoreResult> {
        Ok(PlatformSecretStoreResult::Unavailable)
    }

    #[cfg(target_os = "linux")]
    fn store_to_platform(
        &self,
        account_label: &str,
        payload: &str,
    ) -> Result<PlatformSecretStoreResult> {
        match linux_secret_service::store_password(&self.service_name, account_label, payload)? {
            linux_secret_service::StoreResult::Stored => Ok(PlatformSecretStoreResult::Stored),
            linux_secret_service::StoreResult::Unavailable => {
                self.mark_secure_store_unavailable();
                Err(linux_secret_service::unavailable_error("store"))
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn delete_from_platform(&self, account_label: &str) -> Result<()> {
        if self.secure_store_state() != SecureStoreState::Enabled {
            return Ok(());
        }
        match macos_keychain::delete_password(&self.service_name, account_label) {
            Ok(macos_keychain::KeychainDeleteResult::Deleted) => Ok(()),
            Ok(macos_keychain::KeychainDeleteResult::MissingEntitlement) => {
                self.mark_secure_store_unavailable();
                Ok(())
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    "keychain delete failed; surfacing error so caller can refuse logout"
                );
                Err(error)
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[cfg(not(target_os = "linux"))]
    fn delete_from_platform(&self, _account_label: &str) -> Result<()> {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn delete_from_platform(&self, account_label: &str) -> Result<()> {
        if self.secure_store_state() == SecureStoreState::FileOnly {
            return Ok(());
        }
        match linux_secret_service::delete_password(&self.service_name, account_label)? {
            linux_secret_service::DeleteResult::Deleted => Ok(()),
            linux_secret_service::DeleteResult::Unavailable => {
                self.mark_secure_store_unavailable();
                Err(linux_secret_service::unavailable_error("delete"))
            }
        }
    }

    const fn file_only_load_result() -> PlatformSecretLoadResult {
        PlatformSecretLoadResult::Unavailable {
            requires_existing_file_fallback: false,
        }
    }

    const fn platform_unavailable_load_result() -> PlatformSecretLoadResult {
        PlatformSecretLoadResult::Unavailable {
            requires_existing_file_fallback: cfg!(any(target_os = "macos", target_os = "linux")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PlatformSecretLoadResult, PlatformSecretStore};

    #[test]
    fn disabled_secure_store_allows_file_only_miss() -> crate::Result<()> {
        let store = PlatformSecretStore::new("kodosi-test", std::path::PathBuf::new());
        store.disable_secure_store();

        assert_eq!(
            store.load("alice.device_keys")?,
            PlatformSecretLoadResult::Unavailable {
                requires_existing_file_fallback: false
            }
        );
        Ok(())
    }

    #[test]
    fn unexpected_unavailability_remains_fail_closed() -> crate::Result<()> {
        let store = PlatformSecretStore::new("kodosi-test", std::path::PathBuf::new());
        store.mark_secure_store_unavailable();

        let expected = PlatformSecretLoadResult::Unavailable {
            requires_existing_file_fallback: cfg!(any(target_os = "macos", target_os = "linux")),
        };
        assert_eq!(store.load("alice.device_keys")?, expected);
        assert_eq!(store.load("alice.device_keys")?, expected);
        Ok(())
    }
}
