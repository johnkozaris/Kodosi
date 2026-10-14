#![allow(unsafe_code)]

use std::{
    ffi::OsStr,
    fs,
    os::windows::{
        io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
        process::CommandExt as _,
    },
    path::{Path, PathBuf},
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_INVALID_PARAMETER, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
        WAIT_TIMEOUT,
    },
    System::{
        Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW},
        Threading::{
            CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS, OpenProcess,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        },
    },
    UI::WindowsAndMessaging::AllowSetForegroundWindow,
};

/// The extensions, without dots, that make a file runnable by name.
fn executable_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned())
        .split(';')
        .filter_map(|extension| extension.strip_prefix('.'))
        .filter(|extension| !extension.is_empty())
        .map(str::to_owned)
        .collect()
}

fn present(path: &Path) -> bool {
    // App execution aliases are reparse points that only `symlink_metadata` can read.
    fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_dir())
}

pub(super) fn find_executable(name: &str, paths: &OsStr) -> Option<PathBuf> {
    let extensions = executable_extensions();
    let named = Path::new(name).extension().is_some_and(|extension| {
        extensions
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    });
    std::env::split_paths(paths).find_map(|directory| {
        if named {
            let path = directory.join(name);
            return present(&path).then_some(path);
        }
        extensions
            .iter()
            .map(|extension| directory.join(format!("{name}.{extension}")))
            .find(|path| present(path))
    })
}

pub(super) fn default_shell() -> PathBuf {
    if let Some(pwsh) = super::find_executable("pwsh") {
        return pwsh;
    }
    if let Some(root) = std::env::var_os("SystemRoot") {
        let powershell =
            PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        if powershell.is_file() {
            return powershell;
        }
    }
    std::env::var_os("ComSpec").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from)
}

pub(super) fn detach(command: &mut std::process::Command) {
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    // A child inherits every inheritable handle of its parent, so it would
    // otherwise hold open the pipes of a caller reading this process's output.
    for kind in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: `GetStdHandle` has no pointer arguments.
        let handle = unsafe { GetStdHandle(kind) };
        if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
            // SAFETY: the handle is this process's own; failure leaves it inheritable.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

pub(super) fn is_running(pid: u32) -> bool {
    // SAFETY: `OpenProcess` has no pointer arguments.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return std::io::Error::last_os_error().raw_os_error()
            != Some(ERROR_INVALID_PARAMETER.cast_signed());
    }
    // SAFETY: `OpenProcess` returned an owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    // SAFETY: the handle is open with `SYNCHRONIZE`.
    unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) == WAIT_TIMEOUT }
}

pub(super) fn computer_name() -> Option<String> {
    let mut length = 0u32;
    // SAFETY: a null buffer asks only for the required length.
    unsafe {
        GetComputerNameExW(
            ComputerNameDnsHostname,
            std::ptr::null_mut(),
            &raw mut length,
        )
    };
    let mut buffer = vec![0u16; usize::try_from(length).ok()?];
    // SAFETY: the buffer holds `length` UTF-16 units.
    if unsafe {
        GetComputerNameExW(
            ComputerNameDnsHostname,
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    } == 0
    {
        return None;
    }
    buffer.truncate(usize::try_from(length).ok()?);
    String::from_utf16(&buffer).ok()
}

pub(super) fn allow_foreground(pid: u32) {
    // SAFETY: the call has no pointer arguments; failure only means no grant.
    unsafe { AllowSetForegroundWindow(pid) };
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[test]
    fn a_detached_program_leaves_the_callers_output_alone() {
        const NAME: &str =
            "process::windows::tests::a_detached_program_leaves_the_callers_output_alone";
        const CALLER: &str = "KODOSI_TEST_DETACHING_CALLER";
        if std::env::var_os(CALLER).is_some() {
            let system = std::env::var_os("SystemRoot").unwrap();
            let mut program = Command::new(Path::new(&system).join(r"System32\PING.EXE"));
            program
                .args(["-n", "60", "127.0.0.1"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            super::detach(&mut program);
            let id = program.spawn().unwrap().id();
            writeln!(std::io::stdout(), "\nprogram {id}").unwrap();
            return;
        }
        let mut caller = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CALLER, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let mut output = String::new();
        caller
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();
        let open = started.elapsed();
        assert!(caller.wait().unwrap().success());
        let program = output
            .lines()
            .find_map(|line| line.strip_prefix("program ")?.trim().parse::<u32>().ok())
            .unwrap();
        let running = super::is_running(program);
        drop(
            Command::new("taskkill")
                .args(["/F", "/PID", &program.to_string()])
                .output(),
        );
        assert!(running);
        assert!(
            open < Duration::from_secs(30),
            "the output stayed open for {open:?}"
        );
    }
}
