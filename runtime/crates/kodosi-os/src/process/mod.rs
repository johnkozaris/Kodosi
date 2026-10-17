use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

pub fn find_executable(name: &str) -> Option<PathBuf> {
    find_executable_in(name, &std::env::var_os("PATH")?)
}

pub fn find_executable_in(name: &str, paths: &OsStr) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return None;
    }
    imp::find_executable(name, paths)
}

pub fn default_shell() -> PathBuf {
    imp::default_shell()
}

/// The user session's environment, where Windows or a systemd user manager reports it.
pub fn session_environment() -> Option<Vec<(OsString, OsString)>> {
    imp::session_environment()
}

pub fn detach(command: &mut std::process::Command) {
    imp::detach(command);
}

#[cfg(unix)]
pub fn without_terminal(command: &mut std::process::Command) {
    imp::without_terminal(command);
}

#[cfg(unix)]
pub fn with_default_signals(command: &mut std::process::Command) {
    imp::with_default_signals(command);
}

pub fn is_running(pid: u32) -> bool {
    pid != 0 && imp::is_running(pid)
}

pub fn computer_name() -> Option<String> {
    imp::computer_name()
        .map(|name| name.trim().trim_end_matches(".local").to_owned())
        .filter(|name| !name.is_empty())
}

pub fn host_name() -> Option<String> {
    imp::host_name().filter(|name| !name.is_empty())
}

pub fn machine_id() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let id = std::fs::read_to_string("/etc/machine-id").ok()?;
    let id = id.trim();
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| id.to_owned())
}

pub fn allow_foreground(pid: u32) {
    imp::allow_foreground(pid);
}
