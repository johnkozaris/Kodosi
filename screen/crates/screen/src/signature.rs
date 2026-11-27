//! Whom the host and the helper link with on a Mac.
//!
//! Kodosi Screen holds Screen Recording and Accessibility, so the host and the helper link only
//! with each other: a program with a valid signature of their own team. A development build
//! without a team links with any program, as macOS grants it none of what a released helper holds.

use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_security::{
    SecCSFlags, SecCode, SecRequirement, errSecSuccess, kSecCSSigningInformation,
    kSecCodeInfoTeamIdentifier, kSecGuestAttributeAudit,
};

use crate::handover::AuditToken;

/// Whom a program links with: programs of the team that signed it, or any program.
pub enum Signer {
    Team(String),
    NoTeam,
}

impl Signer {
    /// The team that signed this program, or `None` when that cannot be told, so nothing links.
    ///
    /// macOS reads the signature from the disk when the code object is made, and the validity
    /// check that follows fails if that is not the code that runs.
    pub fn own() -> Option<Self> {
        let mut code = ptr::null_mut();
        // SAFETY: `code` is valid to write.
        let found =
            unsafe { SecCode::copy_self(SecCSFlags::DefaultFlags, NonNull::from(&mut code)) };
        if found != errSecSuccess {
            return None;
        }
        // SAFETY: the call succeeded, so `code` holds a retained code object.
        let code = unsafe { CFRetained::from_raw(NonNull::new(code)?) };
        let mut on_disk = ptr::null();
        // SAFETY: `on_disk` is valid to write.
        let read =
            unsafe { code.copy_static_code(SecCSFlags::DefaultFlags, NonNull::from(&mut on_disk)) };
        if read != errSecSuccess {
            return None;
        }
        // SAFETY: the call succeeded, so `on_disk` holds a retained code object.
        let on_disk = unsafe { CFRetained::from_raw(NonNull::new(on_disk.cast_mut())?) };
        let mut information = ptr::null();
        // SAFETY: `information` is valid to write.
        let described = unsafe {
            SecCode::copy_signing_information(
                &on_disk,
                SecCSFlags(kSecCSSigningInformation),
                NonNull::from(&mut information),
            )
        };
        if described != errSecSuccess {
            return None;
        }
        // SAFETY: the call succeeded, so `information` holds a retained dictionary.
        let information: CFRetained<CFDictionary> =
            unsafe { CFRetained::from_raw(NonNull::new(information.cast_mut())?) };
        // SAFETY: signing information maps strings to Core Foundation objects, and the key is
        // the Security framework's, which lives as long as the program.
        let team = unsafe {
            information
                .cast_unchecked::<CFString, CFType>()
                .get(kSecCodeInfoTeamIdentifier)
        }
        .and_then(|team| team.downcast::<CFString>().ok())
        .map(|team| team.to_string());
        // SAFETY: `code` is a valid code object.
        let valid = unsafe { code.check_validity(SecCSFlags::DefaultFlags, None) };
        (valid == errSecSuccess).then(|| team.map_or(Self::NoTeam, Self::Team))
    }

    /// Whether this program links with the program that `peer` names.
    pub(crate) fn trusts(&self, peer: &AuditToken) -> bool {
        match self {
            Self::Team(team) => signed_by(team, peer),
            Self::NoTeam => true,
        }
    }
}

/// Whether the program that `peer` names runs code with a valid signature of `team`.
fn signed_by(team: &str, peer: &AuditToken) -> bool {
    let text = CFString::from_str(&format!(
        "anchor apple generic and certificate leaf[subject.OU] = \"{team}\""
    ));
    let mut requirement = ptr::null_mut();
    // SAFETY: `requirement` is valid to write.
    let made = unsafe {
        SecRequirement::create_with_string(
            &text,
            SecCSFlags::DefaultFlags,
            NonNull::from(&mut requirement),
        )
    };
    let Some(requirement) = NonNull::new(requirement).filter(|_| made == errSecSuccess) else {
        return false;
    };
    // SAFETY: the call succeeded, so `requirement` is retained.
    let requirement = unsafe { CFRetained::from_raw(requirement) };
    let bytes: Vec<u8> = peer.0.iter().flat_map(|word| word.to_ne_bytes()).collect();
    let token = CFData::from_bytes(&bytes);
    // SAFETY: a key of the Security framework, which lives as long as the program.
    let key = unsafe { kSecGuestAttributeAudit };
    let attributes = CFDictionary::<CFString, CFData>::from_slices(&[key], &[&*token]);
    let mut code = ptr::null_mut();
    // SAFETY: the attributes map the audit key to a token, and `code` is valid to write.
    let found = unsafe {
        SecCode::copy_guest_with_attributes(
            None,
            Some(attributes.as_opaque()),
            SecCSFlags::DefaultFlags,
            NonNull::from(&mut code),
        )
    };
    let Some(code) = NonNull::new(code).filter(|_| found == errSecSuccess) else {
        return false;
    };
    // SAFETY: the call succeeded, so `code` is retained.
    let code = unsafe { CFRetained::from_raw(code) };
    // SAFETY: `code` and `requirement` are valid objects.
    unsafe { code.check_validity(SecCSFlags::DefaultFlags, Some(&requirement)) == errSecSuccess }
}
