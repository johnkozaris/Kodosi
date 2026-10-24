use std::{io, path::Path, time::Duration};

use keyring::{Entry, Error};
use zeroize::Zeroizing;

/// The keyring library opens a session with a Secret Service, which the session bus can
/// start. The library does this once for each process and keeps the result.
pub(super) fn available() -> bool {
    crate::session_bus::within(Duration::from_secs(10), || {
        Some(Entry::store_status().is_ok())
    })
    .unwrap_or(false)
}

pub(super) struct Store {
    service: String,
}

impl Store {
    pub(super) fn new(service: &str, _directory: &Path) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn entry(&self, name: &str) -> io::Result<Entry> {
        Entry::new(&self.service, name)
            .map_err(|error| io::Error::other(format!("Secret Service is unavailable: {error}")))
    }

    pub(super) fn load(&self, name: &str) -> io::Result<Option<Zeroizing<String>>> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(io::Error::other(format!(
                "Secret Service is unavailable: {error}"
            ))),
        }
    }

    pub(super) fn store(&self, name: &str, value: &str) -> io::Result<()> {
        self.entry(name)?.set_password(value).map_err(|error| {
            io::Error::other(format!(
                "Could not store the credential in Secret Service: {error}"
            ))
        })
    }

    pub(super) fn delete(&self, name: &str) -> io::Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(io::Error::other(format!(
                "Could not remove the credential from Secret Service: {error}"
            ))),
        }
    }
}
