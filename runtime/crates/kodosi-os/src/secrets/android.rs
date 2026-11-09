use std::{io, path::Path};

use zeroize::Zeroizing;

pub(super) const fn available() -> bool {
    false
}

pub(super) struct Store;

#[expect(
    clippy::unused_self,
    reason = "the store keeps the methods of the platforms that have one"
)]
impl Store {
    pub(super) const fn new(_service: &str, _directory: &Path) -> Self {
        Self
    }

    pub(super) fn load(&self, _name: &str) -> io::Result<Option<Zeroizing<String>>> {
        Err(unsupported())
    }

    pub(super) fn store(&self, _name: &str, _value: &str) -> io::Result<()> {
        Err(unsupported())
    }

    pub(super) fn delete(&self, _name: &str) -> io::Result<()> {
        Err(unsupported())
    }
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Kodosi for Android keeps its sign-in in private files",
    )
}
