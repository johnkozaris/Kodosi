//! The accounts that the person at the Mac allowed, one item each in the data protection keychain
//! under Kodosi Screen's own access group. macOS lets only a program that Kodosi's team signed
//! with a profile for that group read, add, change or delete them, also before the first answer.

use std::ptr;

use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_security::{
    SecCopyErrorMessageString, SecItemAdd, SecItemCopyMatching, SecItemDelete, errSecDuplicateItem,
    errSecItemNotFound, errSecSuccess, kSecAttrAccessGroup, kSecAttrAccessible,
    kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, kSecAttrAccount, kSecAttrService,
    kSecAttrSynchronizable, kSecClass, kSecClassGenericPassword, kSecMatchLimit, kSecMatchLimitOne,
    kSecUseDataProtectionKeychain, kSecValueData,
};

const SERVICE: &str = "allowed account";

pub(super) struct Answers {
    group: CFRetained<CFString>,
    service: CFRetained<CFString>,
}

impl Answers {
    pub(super) fn new(group: &str) -> Self {
        Self {
            group: CFString::from_str(group),
            service: CFString::from_str(SERVICE),
        }
    }

    pub(super) fn known(&self, key: &str) -> bool {
        let account = CFString::from_str(key);
        // SAFETY: keys and values of the Security framework, which live as long as the program.
        let limit: [(&CFString, &CFType); 2] = unsafe {
            [
                (kSecAttrAccount, &account),
                (kSecMatchLimit, kSecMatchLimitOne),
            ]
        };
        let query = self.query(&limit);
        // SAFETY: the query maps keychain keys to values of their types, and no result is asked.
        unsafe { SecItemCopyMatching(query.as_opaque(), ptr::null_mut()) == errSecSuccess }
    }

    /// Keeps that the person allowed the account that `key` names, on this Mac only.
    pub(super) fn keep(&self, key: &str) -> Result<(), String> {
        let account = CFString::from_str(key);
        let nothing = CFData::from_bytes(&[]);
        // SAFETY: keys and values of the Security framework, which live as long as the program.
        let item: [(&CFString, &CFType); 4] = unsafe {
            [
                (kSecAttrAccount, &account),
                (
                    kSecAttrAccessible,
                    kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
                ),
                (kSecAttrSynchronizable, CFBoolean::new(false)),
                (kSecValueData, &nothing),
            ]
        };
        let item = self.query(&item);
        // SAFETY: the item maps keychain keys to values of their types, and no result is asked.
        let status = unsafe { SecItemAdd(item.as_opaque(), ptr::null_mut()) };
        if status == errSecSuccess || status == errSecDuplicateItem {
            Ok(())
        } else {
            Err(refused("kept no answer", status))
        }
    }

    /// Forgets every account that the person allowed.
    pub(super) fn forget(&self) -> Result<(), String> {
        let query = self.query(&[]);
        // SAFETY: the query maps keychain keys to values of their types.
        let status = unsafe { SecItemDelete(query.as_opaque()) };
        if status == errSecSuccess || status == errSecItemNotFound {
            Ok(())
        } else {
            Err(refused("forgot no answer", status))
        }
    }

    /// Every call names the data protection keychain and the group, so that it never reaches
    /// the login keychain or another group of the team.
    fn query(&self, more: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary<CFString, CFType>> {
        // SAFETY: keys and values of the Security framework, which live as long as the program.
        let mut pairs: Vec<(&CFString, &CFType)> = unsafe {
            vec![
                (kSecClass, kSecClassGenericPassword),
                (kSecUseDataProtectionKeychain, CFBoolean::new(true)),
                (kSecAttrAccessGroup, &self.group),
                (kSecAttrService, &self.service),
            ]
        };
        pairs.extend_from_slice(more);
        let (keys, values): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        CFDictionary::from_slices(&keys, &values)
    }
}

fn refused(what: &str, status: i32) -> String {
    // SAFETY: the reserved argument is null.
    let message = unsafe { SecCopyErrorMessageString(status, ptr::null_mut()) }
        .map_or_else(String::new, |message| message.to_string());
    format!("Kodosi Screen {what}: the keychain answered {status}, {message}")
}
