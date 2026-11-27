//! Kodosi Screen holds Screen Recording and Accessibility, so it serves only Kodosi's host: a
//! program with a valid signature of the helper's own team. A development build without a team
//! serves any host, as macOS grants it none of what a released helper holds.

use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFData, CFDictionary, CFRetained, CFString};
use objc2_security::{
    SecCSFlags, SecCode, SecRequirement, errSecSuccess, kSecCSSigningInformation,
    kSecCodeInfoTeamIdentifier, kSecGuestAttributeAudit,
};
use screen::handover::AuditToken;

use super::value;

enum Signer {
    Team(String),
    NoTeam,
}

pub(super) fn serves(host: &AuditToken) -> bool {
    match own_signer() {
        Some(Signer::Team(team)) => signed_by(&team, host),
        Some(Signer::NoTeam) => true,
        None => false,
    }
}

/// The team that signed this helper, as the kernel loaded it: macOS reads the signature from
/// the app on disk when the code object is made, and the validity check that follows fails if
/// that is not the code that runs, so a helper whose files were swapped serves nobody.
fn own_signer() -> Option<Signer> {
    let mut code = ptr::null_mut();
    // SAFETY: `code` is valid to write.
    let found = unsafe { SecCode::copy_self(SecCSFlags::DefaultFlags, NonNull::from(&mut code)) };
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
    // SAFETY: a key of the Security framework, which lives as long as the program.
    let team = value::<CFString>(&information, unsafe { kSecCodeInfoTeamIdentifier })
        .map(ToString::to_string);
    // SAFETY: `code` is a valid code object.
    let valid = unsafe { code.check_validity(SecCSFlags::DefaultFlags, None) };
    (valid == errSecSuccess).then(|| team.map_or(Signer::NoTeam, Signer::Team))
}

/// Whether the program that `host` names runs code with a valid signature of `team`.
fn signed_by(team: &str, host: &AuditToken) -> bool {
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
    let bytes: Vec<u8> = host.0.iter().flat_map(|word| word.to_ne_bytes()).collect();
    let token = CFData::from_bytes(&bytes);
    // SAFETY: a key of the Security framework, which lives as long as the program.
    let key = unsafe { kSecGuestAttributeAudit };
    let attributes = CFDictionary::<CFString, CFData>::from_slices(&[key], &[&token]);
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

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: i32, info: *mut u32, count: *mut u32) -> i32;
    }

    /// `TASK_AUDIT_TOKEN`.
    const AUDIT_TOKEN: i32 = 15;

    fn own_token() -> AuditToken {
        let mut token = [0; 8];
        let mut count = 8;
        // SAFETY: `token` holds `count` words.
        let read = unsafe {
            task_info(
                mach_task_self_,
                AUDIT_TOKEN,
                token.as_mut_ptr(),
                &raw mut count,
            )
        };
        assert_eq!(read, 0);
        AuditToken(token)
    }

    #[test]
    fn a_team_refuses_a_host_that_it_did_not_sign() {
        assert!(!signed_by("C5886CWK32", &own_token()));
    }
}
