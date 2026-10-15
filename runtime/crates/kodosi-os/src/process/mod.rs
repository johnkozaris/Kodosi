use std::{ffi::OsStr, path::PathBuf};

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

pub fn detach(command: &mut std::process::Command) {
    imp::detach(command);
}

#[cfg(unix)]
pub fn without_terminal(command: &mut std::process::Command) {
    imp::without_terminal(command);
}

pub fn is_running(pid: u32) -> bool {
    pid != 0 && imp::is_running(pid)
}

pub fn computer_name() -> Option<String> {
    imp::computer_name()
        .map(|name| name.trim().trim_end_matches(".local").to_owned())
        .filter(|name| !name.is_empty())
}

pub fn allow_foreground(pid: u32) {
    imp::allow_foreground(pid);
}
