use std::{io, path::Path};

use zeroize::Zeroizing;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use apple as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// The system credential store: Keychain on macOS and iOS, Secret Service on Linux,
/// and files sealed with Windows data protection on Windows.
pub struct SecretStore(imp::Store);

impl SecretStore {
    /// `service` groups the entries. On Windows, `directory` holds the sealed files.
    pub fn new(service: &str, directory: &Path) -> Self {
        Self(imp::Store::new(service, directory))
    }

    pub fn load(&self, name: &str) -> io::Result<Option<Zeroizing<String>>> {
        self.0.load(name)
    }

    pub fn store(&self, name: &str, value: &str) -> io::Result<()> {
        self.0.store(name, value)
    }

    pub fn delete(&self, name: &str) -> io::Result<()> {
        self.0.delete(name)
    }
}
