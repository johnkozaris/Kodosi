use std::{io, path::Path};

use security_framework::passwords::{
    PasswordOptions, delete_generic_password_options, generic_password,
    set_generic_password_options,
};
use zeroize::Zeroizing;

const NOT_FOUND: i32 = -25300;

pub(super) struct Store {
    service: String,
}

impl Store {
    pub(super) fn new(service: &str, _directory: &Path) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn options(&self, name: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(&self.service, name);
        options.use_protected_keychain();
        options
    }

    pub(super) fn load(&self, name: &str) -> io::Result<Option<Zeroizing<String>>> {
        match generic_password(self.options(name)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|value| Some(Zeroizing::new(value)))
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "stored credential is unreadable",
                    )
                }),
            Err(error) if error.code() == NOT_FOUND => Ok(None),
            Err(error) => Err(io::Error::other(format!(
                "Keychain is unavailable: {error}"
            ))),
        }
    }

    pub(super) fn store(&self, name: &str, value: &str) -> io::Result<()> {
        set_generic_password_options(value.as_bytes(), self.options(name)).map_err(|error| {
            io::Error::other(format!(
                "Could not store the credential in Keychain: {error}"
            ))
        })
    }

    pub(super) fn delete(&self, name: &str) -> io::Result<()> {
        match delete_generic_password_options(self.options(name)) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == NOT_FOUND => Ok(()),
            Err(error) => Err(io::Error::other(format!(
                "Could not remove the credential from Keychain: {error}"
            ))),
        }
    }
}
