#![allow(unsafe_code)]

use std::{
    fs, io,
    path::{Path, PathBuf},
    ptr,
};

use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use zeroize::Zeroizing;

use crate::win::Local;

pub(super) const fn available() -> bool {
    true
}

pub(super) struct Store {
    directory: PathBuf,
}

impl Store {
    pub(super) fn new(_service: &str, directory: &Path) -> Self {
        Self {
            directory: directory.to_owned(),
        }
    }

    pub(super) fn load(&self, name: &str) -> io::Result<Option<Zeroizing<String>>> {
        let sealed = match fs::read(self.directory.join(name)) {
            Ok(sealed) => sealed,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let plain = unprotect(&sealed, name.as_bytes())?;
        let text = std::str::from_utf8(&plain).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stored credential is unreadable",
            )
        })?;
        Ok(Some(Zeroizing::new(text.to_owned())))
    }

    pub(super) fn store(&self, name: &str, value: &str) -> io::Result<()> {
        let sealed = protect(value.as_bytes(), name.as_bytes())?;
        crate::fs::write_private(&self.directory.join(name), &sealed)
    }

    pub(super) fn delete(&self, name: &str) -> io::Result<()> {
        match fs::remove_file(self.directory.join(name)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn blob(bytes: &[u8]) -> io::Result<CRYPT_INTEGER_BLOB> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "credential is too large"))?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

fn take(output: &CRYPT_INTEGER_BLOB) -> Zeroizing<Vec<u8>> {
    let _owned = Local(output.pbData.cast());
    let length = output.cbData as usize;
    let bytes =
        Zeroizing::new(unsafe { std::slice::from_raw_parts(output.pbData, length) }.to_vec());
    // SAFETY: the buffer is still allocated and writable until `_owned` drops.
    unsafe { ptr::write_bytes(output.pbData, 0, length) };
    bytes
}

fn protect(plain: &[u8], entropy: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob(plain)?;
    let entropy = blob(entropy)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    if unsafe {
        CryptProtectData(
            &raw const input,
            ptr::null(),
            &raw const entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    } == 0
    {
        return Err(io::Error::other(format!(
            "Windows data protection failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(take(&output).to_vec())
}

fn unprotect(sealed: &[u8], entropy: &[u8]) -> io::Result<Zeroizing<Vec<u8>>> {
    let input = blob(sealed)?;
    let entropy = blob(entropy)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    if unsafe {
        CryptUnprotectData(
            &raw const input,
            ptr::null_mut(),
            &raw const entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    } == 0
    {
        return Err(io::Error::other(format!(
            "Windows could not unseal the stored credential: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(take(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_credentials_round_trip_and_bind_to_their_name() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new("test", root.path());
        assert!(store.load("device").unwrap().is_none());
        store.store("device", "secret").unwrap();
        assert_ne!(fs::read(root.path().join("device")).unwrap(), b"secret");
        assert_eq!(store.load("device").unwrap().unwrap().as_str(), "secret");
        fs::copy(root.path().join("device"), root.path().join("other")).unwrap();
        assert!(store.load("other").is_err());
        store.delete("device").unwrap();
        store.delete("device").unwrap();
        assert!(store.load("device").unwrap().is_none());
    }
}
