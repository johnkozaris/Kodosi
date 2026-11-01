use std::path::Path;

use aes_gcm::{Aes256Gcm, KeyInit as _, Nonce, aead::Aead as _};
use base64::Engine as _;
use serde::Deserialize;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData},
};

use super::{CopyError, chromium::Opener};

const NONCE_BYTES: usize = 12;
const KEY_MARK: &[u8] = b"DPAPI";
const WITH_KEY: &[u8] = b"v10";

#[derive(Deserialize)]
struct LocalState {
    os_crypt: OsCrypt,
}

#[derive(Deserialize)]
struct OsCrypt {
    encrypted_key: String,
}

struct GcmKey(Aes256Gcm);

impl Opener for GcmKey {
    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        let rest = sealed.strip_prefix(WITH_KEY)?;
        let (nonce, body) = rest.split_at_checked(NONCE_BYTES)?;
        self.0.decrypt(Nonce::from_slice(nonce), body).ok()
    }
}

/// Opens a secret that this person's Windows sign-in protects.
#[expect(unsafe_code, reason = "the system's secret store has only a C call")]
fn unprotected(secret: &[u8]) -> Option<Vec<u8>> {
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(secret.len()).ok()?,
        pbData: secret.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: both blobs live across the call, and the system allocates the output.
    let done = unsafe {
        CryptUnprotectData(
            &raw mut input,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &raw mut output,
        )
    };
    if done == 0 {
        return None;
    }
    // SAFETY: on success the output holds `cbData` bytes, which this call must free.
    unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Some(bytes)
    }
}

/// The key of a browser of the Chromium family. A browser with app-bound values keeps those
/// out of reach, and they stay behind.
pub(super) fn opener(name: &str, folder: &Path) -> Result<Box<dyn Opener>, CopyError> {
    let no_access = || CopyError::NoAccess(name.to_owned());
    let root = folder.parent().ok_or_else(no_access)?;
    let state: LocalState = std::fs::read(root.join("Local State"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(no_access)?;
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(state.os_crypt.encrypted_key)
        .map_err(|_| no_access())?;
    let key = sealed
        .strip_prefix(KEY_MARK)
        .and_then(unprotected)
        .ok_or_else(no_access)?;
    let key = Aes256Gcm::new_from_slice(&key).map_err(|_| no_access())?;
    Ok(Box::new(GcmKey(key)))
}
