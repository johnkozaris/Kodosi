#![allow(unsafe_code)]

use std::{
    ffi::{OsStr, c_void},
    io,
    os::windows::{
        ffi::OsStrExt as _,
        io::{FromRawHandle as _, OwnedHandle},
    },
    ptr,
    sync::OnceLock,
};

use windows_sys::Win32::{
    Foundation::{HANDLE, LocalFree},
    Security::{
        ACL,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_OBJECT_TYPE,
        },
        GetSecurityDescriptorDacl, GetTokenInformation, IsWellKnownSid, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
        WinBuiltinAdministratorsSid,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(crate) fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn win32_error(code: u32) -> io::Error {
    io::Error::from_raw_os_error(code.cast_signed())
}

pub(crate) struct Local(pub(crate) *mut c_void);

impl Drop for Local {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

pub(crate) fn user_sid() -> io::Result<&'static str> {
    static SID: OnceLock<Option<String>> = OnceLock::new();
    SID.get_or_init(|| query_user_sid().ok())
        .as_deref()
        .ok_or_else(|| io::Error::other("Windows did not report the current user"))
}

fn query_user_sid() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let handle = std::os::windows::io::AsRawHandle::as_raw_handle(&token);
    let mut length = 0u32;
    unsafe { GetTokenInformation(handle, TokenUser, ptr::null_mut(), 0, &raw mut length) };
    let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
    // SAFETY: the buffer is at least `length` bytes and aligned for `TOKEN_USER`.
    if unsafe {
        GetTokenInformation(
            handle,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &raw mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_string(user.User.Sid)
}

fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text: windows_sys::core::PWSTR = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &raw mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _owned = Local(text.cast());
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text, length)
    }))
}

/// Administrators own the objects that elevated processes create.
pub(crate) fn owned_by_user(handle: HANDLE, kind: SE_OBJECT_TYPE) -> io::Result<bool> {
    let mut owner: PSID = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            handle,
            kind,
            OWNER_SECURITY_INFORMATION,
            &raw mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != 0 {
        return Err(win32_error(status));
    }
    let _descriptor = Local(descriptor);
    if unsafe { IsWellKnownSid(owner, WinBuiltinAdministratorsSid) } != 0 {
        return Ok(true);
    }
    Ok(sid_string(owner)? == user_sid()?)
}

pub(crate) struct Descriptor(Local);

// SAFETY: the descriptor is immutable heap memory owned by this value.
#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "the pointer is an owned, immutable LocalAlloc block"
)]
unsafe impl Send for Descriptor {}
// SAFETY: Windows only reads the descriptor.
unsafe impl Sync for Descriptor {}

impl Descriptor {
    pub(crate) fn private(rights: &str, inherit: bool, system: bool) -> io::Result<Self> {
        let sid = user_sid()?;
        let flags = if inherit { "OICI" } else { "" };
        let system = if system {
            format!("(A;{flags};{rights};;;SY)")
        } else {
            String::new()
        };
        let sddl = format!("D:P(A;{flags};{rights};;;{sid}){system}");
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(&sddl).as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(Local(descriptor)))
    }

    pub(crate) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: 0,
        }
    }

    pub(crate) fn dacl(&self) -> io::Result<*const ACL> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl: *mut ACL = ptr::null_mut();
        if unsafe {
            GetSecurityDescriptorDacl(
                self.0.0,
                &raw mut present,
                &raw mut dacl,
                &raw mut defaulted,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(dacl.cast_const())
    }
}
